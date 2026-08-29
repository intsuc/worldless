from __future__ import annotations

import hashlib
import json

import numpy as np
import pytest

from worldless_transformer import data
from worldless_transformer.spec import DATA_ABI_ID


def test_preprocessing_keeps_short_stories_and_never_crosses_boundaries(
    tmp_path, monkeypatch, tokenizer
) -> None:
    stories = ["a" * 300, "b" * 20]

    def fake_stories(split: str, *, limit: int | None = None):
        assert split == "validation"
        assert limit == 2
        yield from stories

    monkeypatch.setattr(data, "iter_tinystories", fake_stories)
    path = tmp_path / "validation.bin"
    metadata = data.write_token_stream(
        path, split="validation", tokenizer=tokenizer, limit=2
    )

    assert metadata["raw_utf8_byte_count"] == 320
    assert metadata["prediction_count"] == 322
    assert metadata["data_abi_id"] == DATA_ABI_ID
    stream = data.load_token_stream(
        path,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )
    assert stream.windows.tolist() == [
        (0, 0, 256),
        (192, 64, 109),
        (302, 0, 21),
    ]

    inputs, targets, loss_mask = data.sample_batch(
        stream, batch_size=32, generator=np.random.default_rng(3)
    )
    assert inputs.shape == targets.shape == loss_mask.shape == (32, 256)
    assert set(loss_mask.sum(axis=1)).issubset({21, 45, 256})
    for row in inputs:
        assert not (0 in row and 1 in row)

    inputs, targets, loss_mask = data.batch_from_window_indices(
        stream, window_indices=np.asarray([2, 1], dtype=np.int64)
    )
    assert inputs.shape == targets.shape == loss_mask.shape == (2, 256)
    assert loss_mask.sum(axis=1).tolist() == [21, 45]


def test_loader_rejects_the_old_architecture_coupled_data_sidecar(
    tmp_path, monkeypatch, tokenizer
) -> None:
    monkeypatch.setattr(
        data,
        "iter_tinystories",
        lambda split, *, limit=None: iter(["ab"]),
    )
    path = tmp_path / "validation.bin"
    data.write_token_stream(path, split="validation", tokenizer=tokenizer, limit=1)
    sidecar = data.metadata_path(path)
    metadata = json.loads(sidecar.read_text(encoding="utf-8"))
    metadata["architecture_id"] = metadata.pop("data_abi_id")
    sidecar.write_text(json.dumps(metadata), encoding="utf-8")

    with pytest.raises(ValueError, match="invalid schema"):
        data.load_token_stream(
            path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            expected_split="validation",
        )


@pytest.mark.parametrize(
    ("window_indices", "error", "message"),
    [
        ([0], TypeError, "NumPy array"),
        (np.asarray([], dtype=np.int64), ValueError, "must not be empty"),
        (np.asarray([[0]], dtype=np.int64), ValueError, "one-dimensional"),
        (np.asarray([0.0]), TypeError, "integer dtype"),
        (np.asarray([-1], dtype=np.int64), ValueError, "must be in"),
        (np.asarray([1], dtype=np.int64), ValueError, "must be in"),
    ],
)
def test_indexed_batch_rejects_invalid_indices(
    tmp_path, monkeypatch, tokenizer, window_indices, error, message
) -> None:
    monkeypatch.setattr(
        data,
        "iter_tinystories",
        lambda split, *, limit=None: iter(["ab"]),
    )
    path = tmp_path / "validation.bin"
    data.write_token_stream(path, split="validation", tokenizer=tokenizer, limit=1)
    stream = data.load_token_stream(
        path,
        expected_tokenizer_id=tokenizer.tokenizer_id,
        expected_split="validation",
    )

    with pytest.raises(error, match=message):
        data.batch_from_window_indices(stream, window_indices=window_indices)


def _replace_digest(metadata_path, field: str, artifact_path) -> None:
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    metadata[field] = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
    metadata_path.write_text(
        json.dumps(metadata, sort_keys=True, indent=2) + "\n", encoding="utf-8"
    )


def test_loader_rejects_noncanonical_windows_with_a_matching_digest(
    tmp_path, monkeypatch, tokenizer
) -> None:
    monkeypatch.setattr(
        data,
        "iter_tinystories",
        lambda split, *, limit=None: iter(["a" * 300]),
    )
    path = tmp_path / "validation.bin"
    data.write_token_stream(path, split="validation", tokenizer=tokenizer, limit=1)
    window_path = data.windows_path(path)
    windows = np.memmap(window_path, mode="r+", dtype=data.WINDOW_DTYPE)
    windows[1]["start"] = 190
    windows.flush()
    _replace_digest(data.metadata_path(path), "window_sha256", window_path)

    with pytest.raises(ValueError, match="stride"):
        data.load_token_stream(
            path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            expected_split="validation",
        )


def test_loader_rejects_out_of_vocabulary_tokens_with_a_matching_digest(
    tmp_path, monkeypatch, tokenizer
) -> None:
    monkeypatch.setattr(
        data,
        "iter_tinystories",
        lambda split, *, limit=None: iter(["ab"]),
    )
    path = tmp_path / "validation.bin"
    data.write_token_stream(path, split="validation", tokenizer=tokenizer, limit=1)
    tokens = np.memmap(path, mode="r+", dtype=data.TOKEN_DTYPE)
    tokens[1] = 600
    tokens.flush()
    _replace_digest(data.metadata_path(path), "sha256", path)

    with pytest.raises(ValueError, match="token IDs must be"):
        data.load_token_stream(
            path,
            expected_tokenizer_id=tokenizer.tokenizer_id,
            expected_split="validation",
        )
