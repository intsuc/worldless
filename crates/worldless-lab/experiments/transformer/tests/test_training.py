from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import torch

from worldless_transformer import training


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


def test_manifest_records_verified_epoch_and_validation_results(monkeypatch) -> None:
    monkeypatch.setenv("CUBLAS_WORKSPACE_CONFIG", ":4096:8")
    config = training.TrainConfig(
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
        "batch_size": 4,
        "device": "cpu",
        "learning_rate": 0.001,
        "mode": "fake_runtime",
        "seed": 7,
        "validation_batches": 2,
    }
    assert manifest["training"] == {
        "epochs": 1,
        "optimizer_steps": 3,
        "processed_target_count": 123,
        "processed_window_count": 10,
        "window_count": 10,
    }
    assert manifest["validation"] == validation_metrics
