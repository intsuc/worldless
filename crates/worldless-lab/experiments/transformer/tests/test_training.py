from __future__ import annotations

import json
import math
from dataclasses import replace
from types import SimpleNamespace

import numpy as np
import pytest
import torch

from worldless_transformer import training
from worldless_transformer.checkpoint import save_checkpoint
from worldless_transformer.model import Transformer
from worldless_transformer.spec import (
    BASELINE_SPEC,
    EFFICIENT_Q4_SPEC,
    EFFICIENT_SPEC,
)


def test_epoch_batches_cover_every_window_once_with_a_partial_final_batch() -> None:
    def batches(seed: int) -> list[np.ndarray]:
        return [
            batch.copy()
            for batch in training._epoch_window_batches(
                window_count=10,
                batch_size=4,
                generator=np.random.default_rng(seed),
            )
        ]

    first = batches(7)
    assert [len(batch) for batch in first] == [4, 4, 2]
    flattened = np.concatenate(first)
    assert len(np.unique(flattened)) == 10
    assert np.array_equal(np.sort(flattened), np.arange(10))
    second = batches(7)
    assert len(second) == len(first)
    assert all(np.array_equal(left, right) for left, right in zip(first, second))


def test_ordered_evaluation_batches_cover_every_window_once_in_order() -> None:
    batches = list(training._ordered_window_batches(window_count=10, batch_size=4))

    assert [len(batch) for batch in batches] == [4, 4, 2]
    assert np.array_equal(np.concatenate(batches), np.arange(10))


def test_manifest_records_verified_epoch_and_validation_results(monkeypatch) -> None:
    monkeypatch.setenv("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    config = training.TrainConfig(
        architecture="baseline",
        batch_size=4,
        learning_rate=0.001,
        seed=7,
        device="cpu",
        mode="fake_runtime",
        validation_batches=2,
    )
    train_stream = SimpleNamespace(
        metadata={
            "offset_sha256": "train-offsets",
            "sha256": "train-stream",
            "window_count": 10,
            "window_sha256": "train-windows",
        }
    )
    validation_stream = SimpleNamespace(
        metadata={
            "offset_sha256": "validation-offsets",
            "sha256": "validation-stream",
            "window_sha256": "validation-windows",
        }
    )
    validation_metrics = {
        "bits_per_byte": 1.0,
        "eos_accuracy": 0.5,
        "eos_loss": 2.0,
        "loss": 3.0,
        "perplexity": 4.0,
    }

    manifest = training._run_manifest(
        config=config,
        checkpoint_sha256="0" * 64,
        tokenizer_id="tokenizer",
        train_stream=train_stream,
        validation_stream=validation_stream,
        device=torch.device("cpu"),
        optimizer_steps=3,
        processed_window_count=10,
        processed_target_count=123,
        validation_metrics=validation_metrics,
    )

    assert manifest["config"] == {
        "architecture": "baseline",
        "adamw_beta1": 0.9,
        "adamw_beta2": 0.95,
        "adamw_epsilon": 1e-8,
        "adamw_weight_decay": 0.1,
        "attention_logit_denominator": 16,
        "batch_size": 4,
        "device": "cpu",
        "final_learning_rate_fraction": 0.0,
        "learning_rate": 0.001,
        "learning_rate_decay": "cosine",
        "logit_softcap": None,
        "mode": "fake_runtime",
        "seed": 7,
        "validation_batches": 2,
        "warmdown_ratio": None,
        "warmup_ratio": 0.02,
        "warmup_steps": None,
    }
    assert manifest["training"] == {
        "epochs": 1,
        "optimizer_steps": 3,
        "processed_target_count": 123,
        "processed_window_count": 10,
        "window_count": 10,
    }
    assert manifest["validation"] == validation_metrics
    assert manifest["run_schema_version"] == 2
    assert manifest["runtime_abi_compatible"] is True

    for architecture, spec, runtime_compatible in (
        ("efficient", EFFICIENT_SPEC, True),
        ("efficient_q4", EFFICIENT_Q4_SPEC, False),
    ):
        efficient_manifest = training._run_manifest(
            config=replace(config, architecture=architecture),
            checkpoint_sha256="0" * 64,
            tokenizer_id="tokenizer",
            train_stream=train_stream,
            validation_stream=validation_stream,
            device=torch.device("cpu"),
            optimizer_steps=3,
            processed_window_count=10,
            processed_target_count=123,
            validation_metrics=validation_metrics,
        )
        assert efficient_manifest["architecture"] == spec.to_dict()
        assert efficient_manifest["runtime_abi_compatible"] is runtime_compatible

    q4_config = training.TrainConfig(
        architecture="efficient_q4",
        batch_size=4,
        learning_rate=0.001,
        seed=7,
        device="cpu",
        mode="fake_runtime",
        validation_batches=2,
    )
    assert q4_config.attention_logit_denominator == 24
    q4_manifest = training._run_manifest(
        config=q4_config,
        checkpoint_sha256="0" * 64,
        tokenizer_id="tokenizer",
        train_stream=train_stream,
        validation_stream=validation_stream,
        device=torch.device("cpu"),
        optimizer_steps=3,
        processed_window_count=10,
        processed_target_count=123,
        validation_metrics=validation_metrics,
    )
    assert q4_manifest["runtime_abi_compatible"] is True


def test_default_learning_rate_schedule_is_exactly_the_original_schedule() -> None:
    base = 3e-5
    for total_steps in (1, 2, 99, 3_125):
        warmup_steps = max(1, math.ceil(total_steps * 0.02))
        for step in range(1, total_steps + 1):
            if step <= warmup_steps:
                expected = base * step / warmup_steps
            else:
                progress = (step - warmup_steps) / (total_steps - warmup_steps)
                expected = base * 0.5 * (1.0 + math.cos(math.pi * progress))
            assert (
                training._learning_rate(base, step=step, total_steps=total_steps)
                == expected
            )


def test_nanochat_style_schedule_has_absolute_warmup_and_linear_warmdown() -> None:
    schedule = {
        "warmup_ratio": None,
        "warmup_steps": 40,
        "warmdown_ratio": 0.65,
        "final_fraction": 0.05,
        "decay": "linear",
    }

    assert training._learning_rate(1.0, step=1, total_steps=3_125, **schedule) == 0.025
    assert training._learning_rate(1.0, step=40, total_steps=3_125, **schedule) == 1.0
    assert (
        training._learning_rate(1.0, step=1_094, total_steps=3_125, **schedule) == 1.0
    )
    assert training._learning_rate(1.0, step=1_095, total_steps=3_125, **schedule) < 1.0
    assert (
        training._learning_rate(1.0, step=3_125, total_steps=3_125, **schedule) == 0.05
    )


def test_train_config_rejects_ambiguous_warmup_and_invalid_ablation_values() -> None:
    required = {
        "architecture": "baseline",
        "batch_size": 4,
        "learning_rate": 0.001,
        "seed": 7,
        "device": "cpu",
        "mode": "fake_runtime",
        "validation_batches": 2,
    }

    with pytest.raises(ValueError, match="exactly one"):
        training.TrainConfig(**required, warmup_ratio=0.02, warmup_steps=40)
    with pytest.raises(ValueError, match="exactly one"):
        training.TrainConfig(**required, warmup_ratio=None, warmup_steps=None)
    with pytest.raises(ValueError, match="attention logit denominator"):
        training.TrainConfig(**required, attention_logit_denominator=13)
    with pytest.raises(ValueError, match="logit_softcap"):
        training.TrainConfig(**required, logit_softcap=float("inf"))


def test_logit_softcap_is_training_only_and_preserves_raw_argmax() -> None:
    raw_logits = torch.tensor([[-7.0, -1.0, 0.5, 4.0, 12.0]])
    original = raw_logits.clone()

    assert training._apply_logit_softcap(raw_logits, None) is raw_logits
    capped = training._apply_logit_softcap(raw_logits, 15.0)

    assert torch.equal(raw_logits, original)
    assert capped.argmax(dim=-1).tolist() == raw_logits.argmax(dim=-1).tolist()
    assert capped.max() < raw_logits.max()


def _stream_metadata(prefix: str, *, window_count: int | None = None) -> dict:
    value = {
        "offset_sha256": (prefix.encode().hex() + "0" * 64)[:64],
        "sha256": ((prefix + "stream").encode().hex() + "0" * 64)[:64],
        "window_sha256": ((prefix + "windows").encode().hex() + "0" * 64)[:64],
    }
    if window_count is not None:
        value["window_count"] = window_count
    return value


def test_ablation_checkpoint_requires_and_strictly_uses_its_run_manifest(
    tmp_path, monkeypatch, tokenizer
) -> None:
    monkeypatch.setenv("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    config = training.TrainConfig(
        architecture="baseline",
        batch_size=4,
        learning_rate=0.001,
        seed=7,
        device="cpu",
        mode="fake_runtime",
        validation_batches=2,
        attention_logit_denominator=11,
        logit_softcap=15.0,
        warmup_ratio=None,
        warmup_steps=1,
        warmdown_ratio=0.5,
        final_learning_rate_fraction=0.05,
        learning_rate_decay="linear",
    )
    checkpoint_path = tmp_path / "ablation.pt"
    save_checkpoint(
        checkpoint_path,
        model=Transformer(BASELINE_SPEC, attention_logit_denominator=11),
        step=2,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    checkpoint = torch.load(checkpoint_path, weights_only=True)
    assert set(checkpoint) == {
        "architecture_id",
        "attention_logit_denominator",
        "model",
        "schema_version",
        "step",
        "tokenizer_id",
    }
    validation_stream = SimpleNamespace(metadata=_stream_metadata("validation"))

    with pytest.raises(FileNotFoundError, match="run manifest is required"):
        training._load_training_run_checkpoint(
            checkpoint_path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            validation_stream=validation_stream,
        )

    manifest = training._run_manifest(
        config=config,
        checkpoint_sha256=training._file_sha256(checkpoint_path),
        tokenizer_id=tokenizer.tokenizer_id,
        train_stream=SimpleNamespace(
            metadata=_stream_metadata("train", window_count=8)
        ),
        validation_stream=validation_stream,
        device=torch.device("cpu"),
        optimizer_steps=2,
        processed_window_count=8,
        processed_target_count=3,
        validation_metrics={
            "bits_per_byte": 1.0,
            "eos_accuracy": 0.5,
            "eos_loss": 2.0,
            "loss": 3.0,
            "perplexity": 4.0,
        },
    )
    manifest_path = tmp_path / "ablation.pt.run.json"
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

    model, step, loaded_config = training._load_training_run_checkpoint(
        checkpoint_path,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        validation_stream=validation_stream,
    )
    assert model.attention_logit_denominator == 11
    assert step == 2
    assert loaded_config == config

    mismatched_validation = SimpleNamespace(
        metadata={**validation_stream.metadata, "sha256": "f" * 64}
    )
    with pytest.raises(ValueError, match="does not match the validation stream"):
        training._load_training_run_checkpoint(
            checkpoint_path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            validation_stream=mismatched_validation,
        )

    del manifest["config"]["logit_softcap"]
    manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
    with pytest.raises(ValueError, match="config has an invalid schema"):
        training._load_training_run_checkpoint(
            checkpoint_path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            validation_stream=validation_stream,
        )
