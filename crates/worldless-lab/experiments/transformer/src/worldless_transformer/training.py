from __future__ import annotations

import hashlib
import json
import math
import os
import platform
import random
import sys
from collections.abc import Iterator, Mapping
from dataclasses import asdict, dataclass, fields
from pathlib import Path
from typing import Final, Literal

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
from .spec import (
    Architecture,
    exp_q15_table,
    spec_for_architecture,
)
from .tokenizer import GreedyStringPieceTokenizer

_PARAMETER_MIN: Final = -127.0
_PARAMETER_MAX: Final = 127.0
LearningRateDecay = Literal["cosine", "linear"]


@dataclass(frozen=True, slots=True)
class TrainConfig:
    architecture: Architecture
    batch_size: int
    learning_rate: float
    seed: int
    device: str
    mode: ExecutionMode
    validation_batches: int
    attention_logit_denominator: int | None = None
    logit_softcap: float | None = None
    warmup_ratio: float | None = 0.02
    warmup_steps: int | None = None
    warmdown_ratio: float | None = None
    final_learning_rate_fraction: float = 0.0
    learning_rate_decay: LearningRateDecay = "cosine"
    adamw_beta1: float = 0.9
    adamw_beta2: float = 0.95
    adamw_epsilon: float = 1e-8
    adamw_weight_decay: float = 0.1

    def __post_init__(self) -> None:
        spec = spec_for_architecture(self.architecture)
        if self.attention_logit_denominator is None:
            object.__setattr__(
                self,
                "attention_logit_denominator",
                spec.runtime_attention_logit_denominator,
            )
        if (
            not isinstance(self.batch_size, int)
            or isinstance(self.batch_size, bool)
            or self.batch_size <= 0
        ):
            raise ValueError("batch_size must be positive")
        if (
            not isinstance(self.learning_rate, (int, float))
            or isinstance(self.learning_rate, bool)
            or not math.isfinite(self.learning_rate)
            or self.learning_rate <= 0
        ):
            raise ValueError("learning_rate must be finite and positive")
        if self.mode not in ("float", "fake_runtime"):
            raise ValueError("mode must be 'float' or 'fake_runtime'")
        if (
            not isinstance(self.validation_batches, int)
            or isinstance(self.validation_batches, bool)
            or self.validation_batches <= 0
        ):
            raise ValueError("validation_batches must be positive")
        if (
            not isinstance(self.seed, int)
            or isinstance(self.seed, bool)
            or not 0 <= self.seed < 2**32
        ):
            raise ValueError("seed must be in 0..4294967295")
        exp_q15_table(self.attention_logit_denominator)
        if self.logit_softcap is not None and (
            not isinstance(self.logit_softcap, (int, float))
            or isinstance(self.logit_softcap, bool)
            or not math.isfinite(self.logit_softcap)
            or self.logit_softcap <= 0
        ):
            raise ValueError("logit_softcap must be finite and positive when specified")
        if (self.warmup_ratio is None) == (self.warmup_steps is None):
            raise ValueError("exactly one of warmup_ratio and warmup_steps is required")
        if self.warmup_ratio is not None and (
            not isinstance(self.warmup_ratio, (int, float))
            or isinstance(self.warmup_ratio, bool)
            or not math.isfinite(self.warmup_ratio)
            or not 0 <= self.warmup_ratio < 1
        ):
            raise ValueError("warmup_ratio must be finite and in [0, 1)")
        if self.warmup_steps is not None and (
            not isinstance(self.warmup_steps, int)
            or isinstance(self.warmup_steps, bool)
            or self.warmup_steps < 0
        ):
            raise ValueError("warmup_steps must be a non-negative integer")
        if self.warmdown_ratio is not None and (
            not isinstance(self.warmdown_ratio, (int, float))
            or isinstance(self.warmdown_ratio, bool)
            or not math.isfinite(self.warmdown_ratio)
            or not 0 < self.warmdown_ratio <= 1
        ):
            raise ValueError("warmdown_ratio must be finite and in (0, 1]")
        if (
            not isinstance(self.final_learning_rate_fraction, (int, float))
            or isinstance(self.final_learning_rate_fraction, bool)
            or not math.isfinite(self.final_learning_rate_fraction)
            or not 0 <= self.final_learning_rate_fraction <= 1
        ):
            raise ValueError(
                "final_learning_rate_fraction must be finite and in [0, 1]"
            )
        if self.learning_rate_decay not in ("cosine", "linear"):
            raise ValueError("learning_rate_decay must be 'cosine' or 'linear'")
        for name, beta in (
            ("adamw_beta1", self.adamw_beta1),
            ("adamw_beta2", self.adamw_beta2),
        ):
            if (
                not isinstance(beta, (int, float))
                or isinstance(beta, bool)
                or not math.isfinite(beta)
                or not 0 <= beta < 1
            ):
                raise ValueError(f"{name} must be finite and in [0, 1)")
        if (
            not isinstance(self.adamw_epsilon, (int, float))
            or isinstance(self.adamw_epsilon, bool)
            or not math.isfinite(self.adamw_epsilon)
            or self.adamw_epsilon <= 0
        ):
            raise ValueError("adamw_epsilon must be finite and positive")
        if (
            not isinstance(self.adamw_weight_decay, (int, float))
            or isinstance(self.adamw_weight_decay, bool)
            or not math.isfinite(self.adamw_weight_decay)
            or self.adamw_weight_decay < 0
        ):
            raise ValueError("adamw_weight_decay must be finite and non-negative")


@dataclass(frozen=True, slots=True)
class Evaluation:
    loss: float
    bits_per_byte: float
    eos_loss: float
    eos_accuracy: float


@dataclass(slots=True)
class _EvaluationTotals:
    loss_sum: float = 0.0
    target_count: int = 0
    text_loss_sum: float = 0.0
    text_target_count: int = 0
    eos_loss_sum: float = 0.0
    eos_target_count: int = 0
    eos_correct: int = 0

    def add(
        self,
        *,
        logits: torch.Tensor,
        raw_logits: torch.Tensor,
        targets: torch.Tensor,
        loss_mask: torch.Tensor,
        eos_token_id: int,
    ) -> None:
        losses = F.cross_entropy(
            logits.reshape(-1, logits.shape[-1]),
            targets.reshape(-1),
            reduction="none",
        ).view_as(targets)
        text_mask = loss_mask & (targets != eos_token_id)
        eos_mask = loss_mask & (targets == eos_token_id)
        self.loss_sum += float((losses * loss_mask).sum().item())
        self.target_count += int(loss_mask.sum().item())
        self.text_loss_sum += float((losses * text_mask).sum().item())
        self.text_target_count += int(text_mask.sum().item())
        self.eos_loss_sum += float((losses * eos_mask).sum().item())
        self.eos_target_count += int(eos_mask.sum().item())
        self.eos_correct += int(
            ((raw_logits.argmax(dim=-1) == targets) & eos_mask).sum().item()
        )

    def finish(self, stream: TokenStream) -> Evaluation:
        if self.text_target_count == 0 or self.eos_target_count == 0:
            raise ValueError("evaluation must contain both text and EOS targets")
        mean_text_loss = self.text_loss_sum / self.text_target_count
        text_token_count = int(stream.metadata["text_token_count"])
        raw_bytes = int(stream.metadata["raw_utf8_byte_count"])
        return Evaluation(
            loss=self.loss_sum / self.target_count,
            bits_per_byte=(
                mean_text_loss * text_token_count / (raw_bytes * math.log(2.0))
            ),
            eos_loss=self.eos_loss_sum / self.eos_target_count,
            eos_accuracy=self.eos_correct / self.eos_target_count,
        )


_RUN_MANIFEST_SCHEMA_VERSION: Final = 2
_TRAIN_CONFIG_KEYS: Final = frozenset(field.name for field in fields(TrainConfig))
_RUN_MANIFEST_KEYS: Final = frozenset(
    {
        "architecture",
        "checkpoint_sha256",
        "config",
        "deterministic_algorithms",
        "environment",
        "run_schema_version",
        "runtime_abi_compatible",
        "tokenizer_id",
        "train_offsets_sha256",
        "train_stream_sha256",
        "train_windows_sha256",
        "training",
        "validation",
        "validation_offsets_sha256",
        "validation_stream_sha256",
        "validation_windows_sha256",
    }
)
_TRAINING_MANIFEST_KEYS: Final = frozenset(
    {
        "epochs",
        "optimizer_steps",
        "processed_target_count",
        "processed_window_count",
        "window_count",
    }
)
_ENVIRONMENT_MANIFEST_KEYS: Final = frozenset(
    {
        "cublas_workspace_config",
        "cuda",
        "cudnn",
        "device",
        "numpy",
        "platform",
        "python",
        "torch",
    }
)
_VALIDATION_MANIFEST_KEYS: Final = frozenset(
    {"bits_per_byte", "eos_accuracy", "eos_loss", "loss", "perplexity"}
)


def _seed_everything(seed: int) -> None:
    os.environ["CUBLAS_WORKSPACE_CONFIG"] = ":4096:8"
    torch.use_deterministic_algorithms(True)
    torch.backends.cudnn.benchmark = False
    random.seed(seed)
    np.random.seed(seed)
    torch.manual_seed(seed)
    if torch.cuda.is_available():
        torch.cuda.manual_seed_all(seed)


def _learning_rate(
    base: float,
    *,
    step: int,
    total_steps: int,
    warmup_ratio: float | None = 0.02,
    warmup_steps: int | None = None,
    warmdown_ratio: float | None = None,
    final_fraction: float = 0.0,
    decay: LearningRateDecay = "cosine",
) -> float:
    if total_steps <= 0:
        raise ValueError("total_steps must be positive")
    if not 1 <= step <= total_steps:
        raise ValueError("step must be in 1..total_steps")
    if (warmup_ratio is None) == (warmup_steps is None):
        raise ValueError("exactly one of warmup_ratio and warmup_steps is required")
    resolved_warmup_steps = (
        math.ceil(total_steps * warmup_ratio)
        if warmup_ratio is not None
        else warmup_steps
    )
    assert resolved_warmup_steps is not None
    if not 0 <= resolved_warmup_steps <= total_steps:
        raise ValueError("warmup must not exceed total_steps")
    remaining_steps = total_steps - resolved_warmup_steps
    warmdown_steps = (
        remaining_steps
        if warmdown_ratio is None
        else max(1, round(total_steps * warmdown_ratio))
    )
    if warmdown_steps > remaining_steps:
        raise ValueError("warmup and warmdown must not overlap")
    if resolved_warmup_steps > 0 and step <= resolved_warmup_steps:
        return base * step / resolved_warmup_steps
    if remaining_steps == 0:
        return base
    warmdown_start = total_steps - warmdown_steps
    if step <= warmdown_start:
        return base
    progress = (step - warmdown_start) / warmdown_steps
    if decay == "cosine":
        decayed = base * 0.5 * (1.0 + math.cos(math.pi * progress))
    else:
        decayed = base * (1.0 - progress)
    return base * final_fraction + (1.0 - final_fraction) * decayed


def _apply_logit_softcap(logits: torch.Tensor, softcap: float | None) -> torch.Tensor:
    if softcap is None:
        return logits
    return softcap * torch.tanh(logits / softcap)


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
        logits.reshape(-1, logits.shape[-1]),
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


def _ordered_window_batches(
    *, window_count: int, batch_size: int
) -> Iterator[np.ndarray]:
    if window_count <= 0:
        raise ValueError("window_count must be positive")
    if batch_size <= 0:
        raise ValueError("batch_size must be positive")
    for start in range(0, window_count, batch_size):
        yield np.arange(start, min(start + batch_size, window_count), dtype=np.int64)


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
    logit_softcap: float | None = None,
) -> Evaluation:
    if batches <= 0:
        raise ValueError("batches must be positive")
    generator = np.random.default_rng(seed)
    was_training = model.training
    model.eval()
    totals = _EvaluationTotals()
    for _ in range(batches):
        inputs, targets, loss_mask = _torch_batch(
            stream,
            batch_size=batch_size,
            generator=generator,
            device=device,
        )
        raw_logits = model(inputs, mode=mode)
        logits = _apply_logit_softcap(raw_logits, logit_softcap)
        totals.add(
            logits=logits,
            raw_logits=raw_logits,
            targets=targets,
            loss_mask=loss_mask,
            eos_token_id=model.spec.eos_token_id,
        )
    model.train(was_training)
    try:
        return totals.finish(stream)
    except ValueError as error:
        raise ValueError(
            "evaluation sample must contain both text and EOS targets; increase batches"
        ) from error


@torch.no_grad()
def evaluate_all_loss(
    model: Transformer,
    stream: TokenStream,
    *,
    batch_size: int,
    device: torch.device,
    mode: ExecutionMode,
    logit_softcap: float | None = None,
) -> Evaluation:
    was_training = model.training
    model.eval()
    totals = _EvaluationTotals()
    for window_indices in _ordered_window_batches(
        window_count=len(stream.windows), batch_size=batch_size
    ):
        inputs, targets, loss_mask = _torch_batch_from_window_indices(
            stream, window_indices=window_indices, device=device
        )
        raw_logits = model(inputs, mode=mode)
        logits = _apply_logit_softcap(raw_logits, logit_softcap)
        totals.add(
            logits=logits,
            raw_logits=raw_logits,
            targets=targets,
            loss_mask=loss_mask,
            eos_token_id=model.spec.eos_token_id,
        )
    model.train(was_training)
    return totals.finish(stream)


def _run_manifest(
    *,
    config: TrainConfig,
    checkpoint_sha256: str,
    tokenizer_id: str,
    train_stream: TokenStream,
    validation_stream: TokenStream,
    device: torch.device,
    optimizer_steps: int,
    processed_window_count: int,
    processed_target_count: int,
    validation_metrics: dict[str, float],
) -> dict[str, object]:
    _require_sha256(checkpoint_sha256, field="checkpoint_sha256")
    spec = spec_for_architecture(config.architecture)
    selected_device = str(device)
    if device.type == "cuda":
        selected_device = f"{device}:{torch.cuda.get_device_name(device)}"
    return {
        "architecture": spec.to_dict(),
        "checkpoint_sha256": checkpoint_sha256,
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
        "runtime_abi_compatible": (
            spec.data_pack_runtime_compatible
            and config.attention_logit_denominator
            == spec.runtime_attention_logit_denominator
        ),
        "run_schema_version": _RUN_MANIFEST_SCHEMA_VERSION,
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


def _exact_mapping(
    value: object, *, field: str, keys: frozenset[str]
) -> Mapping[str, object]:
    if not isinstance(value, dict) or set(value) != keys:
        raise ValueError(f"training run {field} has an invalid schema")
    return value


def _positive_manifest_integer(value: object, *, field: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise ValueError(f"training run {field} must be a positive integer")
    return value


def _require_sha256(value: object, *, field: str) -> None:
    if (
        not isinstance(value, str)
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise ValueError(f"training run {field} must be a lowercase SHA-256 digest")


def _file_sha256(path: Path) -> str:
    with path.open("rb") as input_file:
        return hashlib.file_digest(input_file, "sha256").hexdigest()


def _load_training_run_checkpoint(
    checkpoint_path: str | Path,
    *,
    expected_tokenizer_id: str,
    validation_stream: TokenStream,
) -> tuple[Transformer, int, TrainConfig]:
    checkpoint_target = Path(checkpoint_path)
    manifest_target = checkpoint_target.with_name(checkpoint_target.name + ".run.json")
    try:
        with manifest_target.open(encoding="utf-8") as input_file:
            value = json.load(input_file)
    except FileNotFoundError as error:
        raise FileNotFoundError(
            f"training run manifest is required: {manifest_target}"
        ) from error
    manifest = _exact_mapping(value, field="manifest", keys=_RUN_MANIFEST_KEYS)
    if manifest["run_schema_version"] != _RUN_MANIFEST_SCHEMA_VERSION:
        raise ValueError(
            f"training run run_schema_version must be {_RUN_MANIFEST_SCHEMA_VERSION}"
        )
    if manifest["tokenizer_id"] != expected_tokenizer_id:
        raise ValueError("training run tokenizer_id does not match the tokenizer")
    if not isinstance(manifest["deterministic_algorithms"], bool):
        raise TypeError("training run deterministic_algorithms must be boolean")
    _exact_mapping(
        manifest["environment"],
        field="environment",
        keys=_ENVIRONMENT_MANIFEST_KEYS,
    )
    for digest_field in (
        "checkpoint_sha256",
        "train_offsets_sha256",
        "train_stream_sha256",
        "train_windows_sha256",
        "validation_offsets_sha256",
        "validation_stream_sha256",
        "validation_windows_sha256",
    ):
        _require_sha256(manifest[digest_field], field=digest_field)
    for manifest_field, metadata_field in (
        ("validation_offsets_sha256", "offset_sha256"),
        ("validation_stream_sha256", "sha256"),
        ("validation_windows_sha256", "window_sha256"),
    ):
        if manifest[manifest_field] != validation_stream.metadata[metadata_field]:
            raise ValueError(
                f"training run {manifest_field} does not match the validation stream"
            )

    raw_config = _exact_mapping(
        manifest["config"], field="config", keys=_TRAIN_CONFIG_KEYS
    )
    try:
        config = TrainConfig(**dict(raw_config))
    except (TypeError, ValueError) as error:
        raise ValueError(f"training run config is invalid: {error}") from error
    spec = spec_for_architecture(config.architecture)
    if manifest["architecture"] != spec.to_dict():
        raise ValueError("training run architecture does not match its config")
    expected_runtime_compatibility = (
        spec.data_pack_runtime_compatible
        and config.attention_logit_denominator
        == spec.runtime_attention_logit_denominator
    )
    if manifest["runtime_abi_compatible"] is not expected_runtime_compatibility:
        raise ValueError(
            "training run runtime_abi_compatible does not match its config"
        )

    training = _exact_mapping(
        manifest["training"], field="training", keys=_TRAINING_MANIFEST_KEYS
    )
    if _positive_manifest_integer(training["epochs"], field="epochs") != 1:
        raise ValueError("training run epochs must be 1")
    optimizer_steps = _positive_manifest_integer(
        training["optimizer_steps"], field="optimizer_steps"
    )
    window_count = _positive_manifest_integer(
        training["window_count"], field="window_count"
    )
    processed_window_count = _positive_manifest_integer(
        training["processed_window_count"], field="processed_window_count"
    )
    if processed_window_count != window_count:
        raise ValueError("training run processed_window_count must equal window_count")
    _positive_manifest_integer(
        training["processed_target_count"], field="processed_target_count"
    )
    expected_optimizer_steps = (
        window_count + config.batch_size - 1
    ) // config.batch_size
    if optimizer_steps != expected_optimizer_steps:
        raise ValueError(
            "training run optimizer_steps does not match window_count and batch_size"
        )
    validation = _exact_mapping(
        manifest["validation"],
        field="validation",
        keys=_VALIDATION_MANIFEST_KEYS,
    )
    if any(
        not isinstance(metric, (int, float))
        or isinstance(metric, bool)
        or not math.isfinite(metric)
        for metric in validation.values()
    ):
        raise ValueError("training run validation metrics must be finite numbers")

    checkpoint_sha256 = _file_sha256(checkpoint_target)
    if checkpoint_sha256 != manifest["checkpoint_sha256"]:
        raise ValueError("checkpoint SHA-256 does not match its training run manifest")
    checkpoint_model, step = load_checkpoint(
        checkpoint_target, expected_tokenizer_id=expected_tokenizer_id
    )
    if checkpoint_model.spec != spec:
        raise ValueError("checkpoint architecture does not match its training run")
    if (
        checkpoint_model.attention_logit_denominator
        != config.attention_logit_denominator
    ):
        raise ValueError(
            "checkpoint attention_logit_denominator does not match its training run"
        )
    if step != optimizer_steps:
        raise ValueError("checkpoint step does not match its training run manifest")
    checkpoint_model.runtime_state()
    return checkpoint_model, step, config


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
    window_count = len(train_stream.windows)
    total_steps = (window_count + config.batch_size - 1) // config.batch_size
    _learning_rate(
        config.learning_rate,
        step=1,
        total_steps=total_steps,
        warmup_ratio=config.warmup_ratio,
        warmup_steps=config.warmup_steps,
        warmdown_ratio=config.warmdown_ratio,
        final_fraction=config.final_learning_rate_fraction,
        decay=config.learning_rate_decay,
    )
    _seed_everything(config.seed)
    device = torch.device(config.device)
    if device.type == "cuda" and not torch.cuda.is_available():
        raise RuntimeError(f"requested CUDA device is unavailable: {config.device}")
    spec = spec_for_architecture(config.architecture)
    model = Transformer(
        spec, attention_logit_denominator=config.attention_logit_denominator
    ).to(device)
    optimizer = torch.optim.AdamW(
        model.parameters(),
        lr=config.learning_rate,
        betas=(config.adamw_beta1, config.adamw_beta2),
        eps=config.adamw_epsilon,
        weight_decay=config.adamw_weight_decay,
    )
    generator = np.random.default_rng(config.seed)
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
            config.learning_rate,
            step=step,
            total_steps=total_steps,
            warmup_ratio=config.warmup_ratio,
            warmup_steps=config.warmup_steps,
            warmdown_ratio=config.warmdown_ratio,
            final_fraction=config.final_learning_rate_fraction,
            decay=config.learning_rate_decay,
        )
        for parameter_group in optimizer.param_groups:
            parameter_group["lr"] = learning_rate
        inputs, targets, loss_mask = _torch_batch_from_window_indices(
            train_stream,
            window_indices=window_indices,
            device=device,
        )
        optimizer.zero_grad(set_to_none=True)
        raw_logits = model(inputs, mode=config.mode)
        logits = _apply_logit_softcap(raw_logits, config.logit_softcap)
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
        logit_softcap=config.logit_softcap,
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
    try:
        manifest = _run_manifest(
            config=config,
            checkpoint_sha256=_file_sha256(checkpoint_target),
            tokenizer_id=tokenizer.tokenizer_id,
            train_stream=train_stream,
            validation_stream=validation_stream,
            device=device,
            optimizer_steps=completed_steps,
            processed_window_count=processed_window_count,
            processed_target_count=processed_target_count,
            validation_metrics=validation_metrics,
        )
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
        "architecture_id": model.spec.architecture_id,
        "batches": batches,
        "checkpoint_step": step,
        "bits_per_byte": evaluation.bits_per_byte,
        "eos_accuracy": evaluation.eos_accuracy,
        "eos_loss": evaluation.eos_loss,
        "loss": evaluation.loss,
        "mode": mode,
        "perplexity": math.exp(min(evaluation.loss, 80.0)),
    }


def evaluate_training_run_checkpoint(
    *,
    tokenizer_path: str | Path,
    validation_tokens: str | Path,
    checkpoint_path: str | Path,
    batch_size: int,
    batches: int,
    seed: int,
    device_name: str,
    mode: ExecutionMode,
) -> dict[str, float | int | str | None]:
    if not 0 <= seed < 2**32:
        raise ValueError("seed must be in 0..4294967295")
    _seed_everything(seed)
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    stream = load_token_stream(
        validation_tokens,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )
    model, step, train_config = _load_training_run_checkpoint(
        checkpoint_path,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        validation_stream=stream,
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
        logit_softcap=train_config.logit_softcap,
    )
    return {
        "architecture_id": model.spec.architecture_id,
        "attention_logit_denominator": train_config.attention_logit_denominator,
        "batches": batches,
        "bits_per_byte": evaluation.bits_per_byte,
        "checkpoint_step": step,
        "eos_accuracy": evaluation.eos_accuracy,
        "eos_loss": evaluation.eos_loss,
        "logit_softcap": train_config.logit_softcap,
        "loss": evaluation.loss,
        "mode": mode,
        "perplexity": math.exp(min(evaluation.loss, 80.0)),
    }


def evaluate_all_training_run_checkpoint(
    *,
    tokenizer_path: str | Path,
    validation_tokens: str | Path,
    checkpoint_path: str | Path,
    batch_size: int,
    device_name: str,
    mode: ExecutionMode,
) -> dict[str, float | int | str | None]:
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    stream = load_token_stream(
        validation_tokens,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )
    model, step, train_config = _load_training_run_checkpoint(
        checkpoint_path,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        validation_stream=stream,
    )
    device = torch.device(device_name)
    if device.type == "cuda" and not torch.cuda.is_available():
        raise RuntimeError(f"requested CUDA device is unavailable: {device_name}")
    model.to(device)
    evaluation = evaluate_all_loss(
        model,
        stream,
        batch_size=batch_size,
        device=device,
        mode=mode,
        logit_softcap=train_config.logit_softcap,
    )
    window_count = len(stream.windows)
    return {
        "architecture_id": model.spec.architecture_id,
        "attention_logit_denominator": train_config.attention_logit_denominator,
        "batches": (window_count + batch_size - 1) // batch_size,
        "bits_per_byte": evaluation.bits_per_byte,
        "checkpoint_step": step,
        "eos_accuracy": evaluation.eos_accuracy,
        "eos_loss": evaluation.eos_loss,
        "logit_softcap": train_config.logit_softcap,
        "loss": evaluation.loss,
        "mode": mode,
        "perplexity": math.exp(min(evaluation.loss, 80.0)),
        "window_count": window_count,
    }
