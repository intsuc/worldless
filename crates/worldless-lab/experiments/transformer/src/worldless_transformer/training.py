from __future__ import annotations

import json
import math
import os
import platform
import random
import sys
from collections.abc import Iterator
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Final

import numpy as np
import torch
import torch.nn.functional as F

from .checkpoint import load_checkpoint, save_checkpoint
from .data import (
    TokenStream,
    batch_from_window_indices,
    load_token_stream,
    sample_batch,
)
from .model import ExecutionMode, Transformer
from .spec import MODEL_SPEC
from .tokenizer import GreedyStringPieceTokenizer

_PARAMETER_MIN: Final = -127.0
_PARAMETER_MAX: Final = 127.0


@dataclass(frozen=True, slots=True)
class TrainConfig:
    batch_size: int
    learning_rate: float
    seed: int
    device: str
    mode: ExecutionMode
    validation_batches: int

    def __post_init__(self) -> None:
        if self.batch_size <= 0:
            raise ValueError("batch_size must be positive")
        if not math.isfinite(self.learning_rate) or self.learning_rate <= 0:
            raise ValueError("learning_rate must be finite and positive")
        if self.mode not in ("float", "fake_runtime"):
            raise ValueError("mode must be 'float' or 'fake_runtime'")
        if self.validation_batches <= 0:
            raise ValueError("validation_batches must be positive")
        if not 0 <= self.seed < 2**32:
            raise ValueError("seed must be in 0..4294967295")


@dataclass(frozen=True, slots=True)
class Evaluation:
    loss: float
    bits_per_byte: float
    eos_loss: float
    eos_accuracy: float


def _seed_everything(seed: int) -> None:
    os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    random.seed(seed)
    np.random.seed(seed)
    torch.manual_seed(seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(seed)


def _learning_rate(base: float, *, step: int, total_steps: int) -> float:
    warmup_steps = max(1, math.ceil(total_steps * 0.02))
    if step <= warmup_steps:
        return base * step / warmup_steps
    decay_steps = total_steps - warmup_steps
    progress = (step - warmup_steps) / decay_steps
    return base * 0.5 * (1.0 + math.cos(math.pi * progress))


def _torch_batch(
    stream: TokenStream,
    *,
    batch_size: int,
    generator: np.random.Generator,
    device: torch.device,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    batch = sample_batch(stream, batch_size=batch_size, generator=generator)
    return _to_device_batch(batch, device=device)


def _torch_batch_from_window_indices(
    stream: TokenStream,
    *,
    window_indices: np.ndarray,
    device: torch.device,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    batch = batch_from_window_indices(stream, window_indices=window_indices)
    return _to_device_batch(batch, device=device)


def _to_device_batch(
    batch: tuple[np.ndarray, np.ndarray, np.ndarray],
    *,
    device: torch.device,
) -> tuple[torch.Tensor, torch.Tensor, torch.Tensor]:
    inputs, targets, loss_mask = batch
    return (
        torch.from_numpy(inputs).to(device=device, non_blocking=True),
        torch.from_numpy(targets).to(device=device, non_blocking=True),
        torch.from_numpy(loss_mask).to(device=device, non_blocking=True),
    )


def _masked_cross_entropy(
    logits: torch.Tensor, targets: torch.Tensor, loss_mask: torch.Tensor
) -> tuple[torch.Tensor, int]:
    losses = F.cross_entropy(
        logits.reshape(-1, MODEL_SPEC.vocab_size),
        targets.reshape(-1),
        reduction="none",
    ).view_as(targets)
    count = int(loss_mask.sum().item())
    if count == 0:
        raise ValueError("sampled batch has no supervised targets")
    return (losses * loss_mask).sum(), count


def _epoch_window_batches(
    *,
    window_count: int,
    batch_size: int,
    generator: np.random.Generator,
) -> Iterator[np.ndarray]:
    order = generator.permutation(window_count)
    for start in range(0, window_count, batch_size):
        yield order[start : start + batch_size]


@torch.no_grad()
def evaluate_loss(
    model: Transformer,
    stream: TokenStream,
    *,
    batch_size: int,
    batches: int,
    seed: int,
    device: torch.device,
    mode: ExecutionMode,
) -> Evaluation:
    if batches <= 0:
        raise ValueError("batches must be positive")
    generator = np.random.default_rng(seed)
    was_training = model.training
    model.eval()
    loss_sum = 0.0
    target_count = 0
    text_loss_sum = 0.0
    text_target_count = 0
    eos_loss_sum = 0.0
    eos_target_count = 0
    eos_correct = 0
    for _ in range(batches):
        inputs, targets, loss_mask = _torch_batch(
            stream,
            batch_size=batch_size,
            generator=generator,
            device=device,
        )
        logits = model(inputs, mode=mode)
        losses = F.cross_entropy(
            logits.reshape(-1, MODEL_SPEC.vocab_size),
            targets.reshape(-1),
            reduction="none",
        ).view_as(targets)
        text_mask = loss_mask & (targets != MODEL_SPEC.eos_token_id)
        eos_mask = loss_mask & (targets == MODEL_SPEC.eos_token_id)
        loss_sum += float((losses * loss_mask).sum().item())
        target_count += int(loss_mask.sum().item())
        text_loss_sum += float((losses * text_mask).sum().item())
        text_target_count += int(text_mask.sum().item())
        eos_loss_sum += float((losses * eos_mask).sum().item())
        eos_target_count += int(eos_mask.sum().item())
        eos_correct += int(((logits.argmax(dim=-1) == targets) & eos_mask).sum().item())
    model.train(was_training)
    if text_target_count == 0 or eos_target_count == 0:
        raise ValueError(
            "evaluation sample must contain both text and EOS targets; increase batches"
        )
    mean_loss = loss_sum / target_count
    mean_text_loss = text_loss_sum / text_target_count
    text_token_count = int(stream.metadata["text_token_count"])
    raw_bytes = int(stream.metadata["raw_utf8_byte_count"])
    bits_per_byte = mean_text_loss * text_token_count / (raw_bytes * math.log(2.0))
    return Evaluation(
        loss=mean_loss,
        bits_per_byte=bits_per_byte,
        eos_loss=eos_loss_sum / eos_target_count,
        eos_accuracy=eos_correct / eos_target_count,
    )


def _run_manifest(
    *,
    config: TrainConfig,
    tokenizer_id: str,
    train_stream: TokenStream,
    validation_stream: TokenStream,
    device: torch.device,
    optimizer_steps: int,
    processed_window_count: int,
    processed_target_count: int,
    validation_metrics: dict[str, float],
) -> dict[str, object]:
    selected_device = str(device)
    if device.type == "cuda":
        selected_device = f"{device}:{torch.cuda.get_device_name(device)}"
    return {
        "architecture": MODEL_SPEC.to_dict(),
        "config": asdict(config),
        "deterministic_algorithms": torch.are_deterministic_algorithms_enabled(),
        "environment": {
            "cublas_workspace_config": os.environ["CUBLAS_WORKSPACE_CONFIG"],
            "cuda": torch.version.cuda,
            "cudnn": torch.backends.cudnn.version(),
            "device": selected_device,
            "numpy": np.__version__,
            "platform": platform.platform(),
            "python": sys.version,
            "torch": torch.__version__,
        },
        "training": {
            "epochs": 1,
            "optimizer_steps": optimizer_steps,
            "processed_target_count": processed_target_count,
            "processed_window_count": processed_window_count,
            "window_count": int(train_stream.metadata["window_count"]),
        },
        "validation": validation_metrics,
        "tokenizer_id": tokenizer_id,
        "train_offsets_sha256": train_stream.metadata["offset_sha256"],
        "train_stream_sha256": train_stream.metadata["sha256"],
        "train_windows_sha256": train_stream.metadata["window_sha256"],
        "validation_offsets_sha256": validation_stream.metadata["offset_sha256"],
        "validation_stream_sha256": validation_stream.metadata["sha256"],
        "validation_windows_sha256": validation_stream.metadata["window_sha256"],
    }


def _validation_metrics(evaluation: Evaluation) -> dict[str, float]:
    return {
        "bits_per_byte": evaluation.bits_per_byte,
        "eos_accuracy": evaluation.eos_accuracy,
        "eos_loss": evaluation.eos_loss,
        "loss": evaluation.loss,
        "perplexity": math.exp(min(evaluation.loss, 80.0)),
    }


def train(
    *,
    tokenizer_path: str | Path,
    train_tokens: str | Path,
    validation_tokens: str | Path,
    output_checkpoint: str | Path,
    config: TrainConfig,
) -> None:
    checkpoint_target = Path(output_checkpoint)
    manifest_target = checkpoint_target.with_name(checkpoint_target.name + ".run.json")
    if checkpoint_target.exists() or manifest_target.exists():
        raise FileExistsError(
            f"refusing to replace checkpoint artifacts: {checkpoint_target}, {manifest_target}"
        )
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    train_stream = load_token_stream(
        train_tokens,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="train",
    )
    validation_stream = load_token_stream(
        validation_tokens,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )
    _seed_everything(config.seed)
    device = torch.device(config.device)
    if device.type == "cuda" and not torch.cuda.is_available():
        raise RuntimeError(f"requested CUDA device is unavailable: {config.device}")
    model = Transformer().to(device)
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=config.learning_rate,
        betas=(0.9, 0.95),
        weight_decay=0.1,
    )
    generator = np.random.default_rng(config.seed)
    window_count = len(train_stream.windows)
    total_steps = (window_count + config.batch_size - 1) // config.batch_size
    completed_steps = 0
    processed_window_count = 0
    processed_target_count = 0
    model.train()
    epoch_batches = _epoch_window_batches(
        window_count=window_count,
        batch_size=config.batch_size,
        generator=generator,
    )
    for step, window_indices in enumerate(epoch_batches, start=1):
        learning_rate = _learning_rate(
            config.learning_rate, step=step, total_steps=total_steps
        )
        for parameter_group in optimizer.param_groups:
            parameter_group["lr"] = learning_rate
        inputs, targets, loss_mask = _torch_batch_from_window_indices(
            train_stream,
            window_indices=window_indices,
            device=device,
        )
        optimizer.zero_grad(set_to_none=True)
        logits = model(inputs, mode=config.mode)
        loss_sum, target_count = _masked_cross_entropy(logits, targets, loss_mask)
        loss = loss_sum / target_count
        if not torch.isfinite(loss):
            raise RuntimeError(f"non-finite training loss at step {step}")
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), max_norm=1.0)
        optimizer.step()
        with torch.no_grad():
            for parameter in model.parameters():
                parameter.clamp_(_PARAMETER_MIN, _PARAMETER_MAX)
        processed_window_count += len(window_indices)
        processed_target_count += target_count
        completed_steps = step
        if step == 1 or step % 1_000 == 0 or step == total_steps:
            print(
                json.dumps(
                    {
                        "learning_rate": learning_rate,
                        "step": step,
                        "train_loss": float(loss.detach().item()),
                    },
                    sort_keys=True,
                ),
                flush=True,
            )

    if completed_steps != total_steps:
        raise AssertionError(
            "one epoch must execute the derived number of optimizer steps: "
            f"expected {total_steps}, got {completed_steps}"
        )
    if processed_window_count != window_count:
        raise AssertionError(
            "one epoch must process every training window exactly once: "
            f"expected {window_count}, got {processed_window_count}"
        )
    expected_target_count = int(train_stream.metadata["prediction_count"])
    if processed_target_count != expected_target_count:
        raise AssertionError(
            "one epoch must process every supervised target exactly once: "
            f"expected {expected_target_count}, got {processed_target_count}"
        )

    validation = evaluate_loss(
        model,
        validation_stream,
        batch_size=config.batch_size,
        batches=config.validation_batches,
        seed=(config.seed + 1) % 2**32,
        device=device,
        mode=config.mode,
    )
    validation_metrics = _validation_metrics(validation)
    print(
        json.dumps(
            {
                "step": total_steps,
                **{
                    f"validation_{field}": value
                    for field, value in validation_metrics.items()
                },
            },
            sort_keys=True,
        ),
        flush=True,
    )
    save_checkpoint(
        checkpoint_target,
        model=model.cpu(),
        step=completed_steps,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    manifest = _run_manifest(
        config=config,
        tokenizer_id=tokenizer.tokenizer_id,
        train_stream=train_stream,
        validation_stream=validation_stream,
        device=device,
        optimizer_steps=completed_steps,
        processed_window_count=processed_window_count,
        processed_target_count=processed_target_count,
        validation_metrics=validation_metrics,
    )
    try:
        with manifest_target.open("x", encoding="utf-8") as output:
            output.write(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    except BaseException:
        checkpoint_target.unlink(missing_ok=True)
        manifest_target.unlink(missing_ok=True)
        raise


def evaluate_checkpoint(
    *,
    tokenizer_path: str | Path,
    validation_tokens: str | Path,
    checkpoint_path: str | Path,
    batch_size: int,
    batches: int,
    seed: int,
    device_name: str,
    mode: ExecutionMode,
) -> dict[str, float | int | str]:
    if not 0 <= seed < 2**32:
        raise ValueError("seed must be in 0..4294967295")
    _seed_everything(seed)
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    stream = load_token_stream(
        validation_tokens,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )
    model, step = load_checkpoint(
        checkpoint_path, expected_tokenizer_id=tokenizer.tokenizer_id
    )
    device = torch.device(device_name)
    if device.type == "cuda" and not torch.cuda.is_available():
        raise RuntimeError(f"requested CUDA device is unavailable: {device_name}")
    model.to(device)
    evaluation = evaluate_loss(
        model,
        stream,
        batch_size=batch_size,
        batches=batches,
        seed=seed,
        device=device,
        mode=mode,
    )
    return {
        "batches": batches,
        "checkpoint_step": step,
        "bits_per_byte": evaluation.bits_per_byte,
        "eos_accuracy": evaluation.eos_accuracy,
        "eos_loss": evaluation.eos_loss,
        "loss": evaluation.loss,
        "mode": mode,
        "perplexity": math.exp(min(evaluation.loss, 80.0)),
    }
