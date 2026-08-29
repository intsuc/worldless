from __future__ import annotations

import hashlib
import json
import os
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path
from typing import Final

import numpy as np
import pyarrow as pa
import pyarrow.parquet as pq
from huggingface_hub import HfFileSystem

from .spec import ARCHITECTURE_ID, MODEL_SPEC, SCHEMA_VERSION
from .tokenizer import GreedyStringPieceTokenizer

DATASET_ID: Final = "roneneldan/TinyStories"
DATASET_REVISION: Final = "f54c09fd23315a6f9c86f9dc80f725de7d8f9c64"
DATASET_SPLITS: Final = frozenset({"train", "validation"})
DATASET_ROW_COUNTS: Final = {"train": 2_119_719, "validation": 21_990}
DATASET_SHARD_COUNTS: Final = {"train": 4, "validation": 1}
TEXT_COLUMN: Final = "text"
TOKEN_DTYPE: Final = np.dtype("<u2")
WINDOW_DTYPE: Final = np.dtype(
    [("start", "<u8"), ("loss_start", "<u2"), ("loss_end", "<u2")]
)
WINDOW_DTYPE_NAME: Final = (
    "struct<uint64-le:start:uint16-le:loss_start:uint16-le:loss_end>"
)
WINDOW_STRIDE: Final = MODEL_SPEC.context_length - MODEL_SPEC.attention_window
_STREAM_METADATA_KEYS: Final = {
    "architecture_id",
    "dataset_id",
    "dataset_revision",
    "dtype",
    "offset_count",
    "offset_dtype",
    "offset_sha256",
    "prediction_count",
    "raw_utf8_byte_count",
    "schema_version",
    "sha256",
    "split",
    "story_count",
    "token_count",
    "tokenizer_id",
    "text_token_count",
    "window_count",
    "window_dtype",
    "window_sha256",
}


def _validated_dataset_files(split: str) -> tuple[HfFileSystem, list[str]]:
    filesystem = HfFileSystem()
    pattern = f"datasets/{DATASET_ID}@{DATASET_REVISION}/data/{split}-*.parquet"
    paths = sorted(filesystem.glob(pattern))
    expected_shards = DATASET_SHARD_COUNTS[split]
    if len(paths) != expected_shards:
        raise ValueError(
            f"TinyStories {split} must have {expected_shards} parquet shards, "
            f"got {len(paths)}"
        )
    row_count = 0
    for path in paths:
        with filesystem.open(path, "rb") as source:
            parquet = pq.ParquetFile(source)
            schema = parquet.schema_arrow
            if (
                schema.names != [TEXT_COLUMN]
                or schema.field(TEXT_COLUMN).type != pa.string()
            ):
                raise ValueError(
                    "TinyStories parquet schema changed: expected one string field "
                    f"'text', got {schema}"
                )
            row_count += parquet.metadata.num_rows
    expected_rows = DATASET_ROW_COUNTS[split]
    if row_count != expected_rows:
        raise ValueError(
            f"TinyStories {split} row count must be {expected_rows}, got {row_count}"
        )
    return filesystem, paths


def iter_tinystories(split: str, *, limit: int | None = None) -> Iterator[str]:
    if split not in DATASET_SPLITS:
        raise ValueError(
            f"split must be one of {sorted(DATASET_SPLITS)}, got {split!r}"
        )
    if limit is not None and limit <= 0:
        raise ValueError("limit must be positive when specified")
    filesystem, paths = _validated_dataset_files(split)
    emitted = 0
    for path in paths:
        with filesystem.open(path, "rb") as source:
            parquet = pq.ParquetFile(source)
            for batch in parquet.iter_batches(
                batch_size=1_024, columns=[TEXT_COLUMN], use_threads=False
            ):
                for scalar in batch.column(0):
                    text = scalar.as_py()
                    if not isinstance(text, str):
                        raise TypeError(
                            f"TinyStories row {emitted} text is not a string"
                        )
                    yield text
                    emitted += 1
                    if limit is not None and emitted == limit:
                        return
    if emitted != DATASET_ROW_COUNTS[split]:
        raise ValueError(
            f"TinyStories {split} yielded {emitted} rows, "
            f"expected {DATASET_ROW_COUNTS[split]}"
        )


def metadata_path(token_path: str | Path) -> Path:
    path = Path(token_path)
    return path.with_name(path.name + ".json")


def offsets_path(token_path: str | Path) -> Path:
    path = Path(token_path)
    return path.with_name(path.name + ".offsets")


def windows_path(token_path: str | Path) -> Path:
    path = Path(token_path)
    return path.with_name(path.name + ".windows")


def _write_all(file_descriptor: int, payload: bytes) -> None:
    remaining = memoryview(payload)
    while remaining:
        written = os.write(file_descriptor, remaining)
        if written == 0:
            raise OSError("zero-byte write while producing token stream")
        remaining = remaining[written:]


def write_token_stream(
    output: str | Path,
    *,
    split: str,
    tokenizer: GreedyStringPieceTokenizer,
    limit: int | None = None,
) -> dict[str, object]:
    target = Path(output)
    sidecar = metadata_path(target)
    offsets_target = offsets_path(target)
    windows_target = windows_path(target)
    if (
        target.exists()
        or sidecar.exists()
        or offsets_target.exists()
        or windows_target.exists()
    ):
        raise FileExistsError(
            "refusing to replace token stream artifacts: "
            f"{target}, {offsets_target}, {windows_target}, {sidecar}"
        )
    target.parent.mkdir(parents=True, exist_ok=True)
    file_descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    offsets_descriptor = os.open(
        offsets_target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644
    )
    windows_descriptor = os.open(
        windows_target, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644
    )
    digest = hashlib.sha256()
    offset_digest = hashlib.sha256()
    window_digest = hashlib.sha256()
    token_count = 0
    text_token_count = 0
    story_count = 0
    prediction_count = 0
    raw_utf8_byte_count = 0
    window_count = 0
    try:
        initial_offset = np.asarray([0], dtype="<u8").tobytes()
        _write_all(offsets_descriptor, initial_offset)
        offset_digest.update(initial_offset)
        for text in iter_tinystories(split, limit=limit):
            tokens = np.asarray(tokenizer.encode_story(text), dtype=TOKEN_DTYPE)
            encoded = tokens.tobytes(order="C")
            _write_all(file_descriptor, encoded)
            digest.update(encoded)
            story_start = token_count
            token_count += len(tokens)
            text_token_count += len(tokens) - 2
            story_count += 1
            prediction_count += len(tokens) - 1
            raw_utf8_byte_count += len(text.encode("utf-8"))
            encoded_offset = np.asarray([token_count], dtype="<u8").tobytes()
            _write_all(offsets_descriptor, encoded_offset)
            offset_digest.update(encoded_offset)
            local_start = 0
            while local_start < len(tokens) - 1:
                loss_start = 0 if local_start == 0 else MODEL_SPEC.attention_window
                loss_end = min(MODEL_SPEC.context_length, len(tokens) - 1 - local_start)
                if loss_start < loss_end:
                    window = np.asarray(
                        [(story_start + local_start, loss_start, loss_end)],
                        dtype=WINDOW_DTYPE,
                    ).tobytes()
                    _write_all(windows_descriptor, window)
                    window_digest.update(window)
                    window_count += 1
                local_start += WINDOW_STRIDE
    except BaseException:
        os.close(file_descriptor)
        os.close(offsets_descriptor)
        os.close(windows_descriptor)
        target.unlink(missing_ok=True)
        offsets_target.unlink(missing_ok=True)
        windows_target.unlink(missing_ok=True)
        raise
    else:
        os.close(file_descriptor)
        os.close(offsets_descriptor)
        os.close(windows_descriptor)

    if story_count == 0:
        target.unlink()
        offsets_target.unlink()
        windows_target.unlink()
        raise ValueError("TinyStories stream produced no stories")
    expected_story_count = DATASET_ROW_COUNTS[split] if limit is None else limit
    if story_count != expected_story_count:
        target.unlink()
        offsets_target.unlink()
        windows_target.unlink()
        raise ValueError(
            f"TinyStories {split} yielded {story_count} rows, expected {expected_story_count}"
        )
    metadata: dict[str, object] = {
        "architecture_id": ARCHITECTURE_ID,
        "dataset_id": DATASET_ID,
        "dataset_revision": DATASET_REVISION,
        "dtype": "uint16-le",
        "offset_count": story_count + 1,
        "offset_dtype": "uint64-le",
        "offset_sha256": offset_digest.hexdigest(),
        "prediction_count": prediction_count,
        "raw_utf8_byte_count": raw_utf8_byte_count,
        "schema_version": SCHEMA_VERSION,
        "sha256": digest.hexdigest(),
        "split": split,
        "story_count": story_count,
        "token_count": token_count,
        "tokenizer_id": tokenizer.tokenizer_id,
        "text_token_count": text_token_count,
        "window_count": window_count,
        "window_dtype": WINDOW_DTYPE_NAME,
        "window_sha256": window_digest.hexdigest(),
    }
    try:
        sidecar.write_text(
            json.dumps(metadata, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
    except BaseException:
        target.unlink(missing_ok=True)
        offsets_target.unlink(missing_ok=True)
        windows_target.unlink(missing_ok=True)
        sidecar.unlink(missing_ok=True)
        raise
    return metadata


@dataclass(frozen=True, slots=True)
class TokenStream:
    tokens: np.memmap
    offsets: np.memmap
    windows: np.memmap
    metadata: dict[str, object]


def load_token_stream(
    token_path: str | Path,
    *,
    expected_tokenizer_id: str,
    expected_split: str,
) -> TokenStream:
    path = Path(token_path)
    sidecar = metadata_path(path)
    offset_file = offsets_path(path)
    window_file = windows_path(path)
    value = json.loads(sidecar.read_text(encoding="utf-8"))
    if not isinstance(value, dict) or set(value) != _STREAM_METADATA_KEYS:
        raise ValueError("token stream metadata has an invalid schema")
    expected = {
        "architecture_id": ARCHITECTURE_ID,
        "dataset_id": DATASET_ID,
        "dataset_revision": DATASET_REVISION,
        "dtype": "uint16-le",
        "offset_dtype": "uint64-le",
        "schema_version": SCHEMA_VERSION,
        "split": expected_split,
        "tokenizer_id": expected_tokenizer_id,
    }
    for field, required in expected.items():
        if value[field] != required:
            raise ValueError(
                f"token stream {field} must be {required!r}, got {value[field]!r}"
            )
    token_count = value["token_count"]
    story_count = value["story_count"]
    offset_count = value["offset_count"]
    window_count = value["window_count"]
    prediction_count = value["prediction_count"]
    raw_utf8_byte_count = value["raw_utf8_byte_count"]
    text_token_count = value["text_token_count"]
    if (
        not isinstance(token_count, int)
        or isinstance(token_count, bool)
        or token_count <= 0
    ):
        raise ValueError("token_count must be a positive integer")
    if (
        not isinstance(story_count, int)
        or isinstance(story_count, bool)
        or story_count <= 0
    ):
        raise ValueError("story_count must be a positive integer")
    if offset_count != story_count + 1:
        raise ValueError("offset_count must equal story_count + 1")
    for field, count in (
        ("window_count", window_count),
        ("prediction_count", prediction_count),
        ("raw_utf8_byte_count", raw_utf8_byte_count),
        ("text_token_count", text_token_count),
    ):
        if not isinstance(count, int) or isinstance(count, bool) or count <= 0:
            raise ValueError(f"{field} must be a positive integer")
    if prediction_count != text_token_count + story_count:
        raise ValueError("prediction_count must equal text_token_count + story_count")
    if text_token_count != token_count - 2 * story_count:
        raise ValueError(
            "text_token_count must exclude exactly one BOS and EOS per story"
        )
    if value["window_dtype"] != WINDOW_DTYPE_NAME:
        raise ValueError("window_dtype does not match the fixed window schema")
    if path.stat().st_size != token_count * TOKEN_DTYPE.itemsize:
        raise ValueError("token stream byte length does not match token_count")
    digest = _file_sha256(path)
    if value["sha256"] != digest:
        raise ValueError("token stream SHA-256 does not match metadata")
    if offset_file.stat().st_size != offset_count * np.dtype("<u8").itemsize:
        raise ValueError("offset byte length does not match offset_count")
    if value["offset_sha256"] != _file_sha256(offset_file):
        raise ValueError("offset SHA-256 does not match metadata")
    if window_file.stat().st_size != window_count * WINDOW_DTYPE.itemsize:
        raise ValueError("window byte length does not match window_count")
    if value["window_sha256"] != _file_sha256(window_file):
        raise ValueError("window SHA-256 does not match metadata")
    tokens = np.memmap(path, mode="r", dtype=TOKEN_DTYPE, shape=(token_count,))
    offsets = np.memmap(offset_file, mode="r", dtype="<u8", shape=(offset_count,))
    if (
        offsets[0] != 0
        or offsets[-1] != token_count
        or np.any(offsets[1:] <= offsets[:-1])
    ):
        raise ValueError(
            "story offsets must be strictly increasing from zero to token_count"
        )
    if np.any(offsets[1:] - offsets[:-1] < 2):
        raise ValueError("every story must contain at least BOS and EOS")
    _validate_tokens(tokens, offsets)
    windows = np.memmap(
        window_file, mode="r", dtype=WINDOW_DTYPE, shape=(window_count,)
    )
    starts = windows["start"].astype(np.int64)
    loss_starts = windows["loss_start"].astype(np.int64)
    loss_ends = windows["loss_end"].astype(np.int64)
    if np.any(starts[1:] <= starts[:-1]):
        raise ValueError("window starts must be strictly increasing")
    if np.any((loss_starts < 0) | (loss_starts >= loss_ends)):
        raise ValueError("every window must contain at least one loss position")
    if np.any(loss_ends > MODEL_SPEC.context_length):
        raise ValueError("window loss_end exceeds context length")
    if np.any((loss_starts != 0) & (loss_starts != MODEL_SPEC.attention_window)):
        raise ValueError("window loss_start must be zero or the attention window")
    story_indices = np.searchsorted(offsets, starts, side="right") - 1
    if np.any(story_indices < 0) or np.any(story_indices >= story_count):
        raise ValueError("window start lies outside all stories")
    story_ends = offsets[story_indices + 1].astype(np.int64)
    if np.any(starts + loss_ends >= story_ends):
        raise ValueError("window target crosses a story boundary")
    first_in_story = np.empty(window_count, dtype=np.bool_)
    first_in_story[0] = True
    first_in_story[1:] = story_indices[1:] != story_indices[:-1]
    if not np.array_equal(
        story_indices[first_in_story], np.arange(story_count, dtype=np.int64)
    ):
        raise ValueError("every story must have exactly one first window")
    if not np.array_equal(starts[first_in_story], offsets[:-1].astype(np.int64)):
        raise ValueError("each story's first window must start at its BOS token")
    continuation_indices = np.flatnonzero(~first_in_story)
    if np.any(
        starts[continuation_indices] - starts[continuation_indices - 1] != WINDOW_STRIDE
    ):
        raise ValueError(f"continuation windows must use stride {WINDOW_STRIDE}")
    expected_loss_starts = np.where(first_in_story, 0, MODEL_SPEC.attention_window)
    if not np.array_equal(loss_starts, expected_loss_starts):
        raise ValueError("window loss_start does not match its canonical position")
    expected_loss_ends = np.minimum(MODEL_SPEC.context_length, story_ends - starts - 1)
    if not np.array_equal(loss_ends, expected_loss_ends):
        raise ValueError("window loss_end does not match its story boundary")
    last_in_story = np.empty(window_count, dtype=np.bool_)
    last_in_story[:-1] = story_indices[:-1] != story_indices[1:]
    last_in_story[-1] = True
    if np.any(
        starts[last_in_story] + loss_ends[last_in_story]
        != story_ends[last_in_story] - 1
    ):
        raise ValueError("each story's final window must cover its final prediction")
    if int(np.sum(loss_ends - loss_starts, dtype=np.int64)) != prediction_count:
        raise ValueError("window loss positions do not cover prediction_count exactly")
    return TokenStream(
        tokens=tokens,
        offsets=offsets,
        windows=windows,
        metadata=value,
    )


def _validate_tokens(tokens: np.memmap, offsets: np.memmap) -> None:
    expected_bos = offsets[:-1]
    expected_eos = offsets[1:] - 1
    bos_cursor = 0
    eos_cursor = 0
    chunk_size = 1 << 20
    for start in range(0, len(tokens), chunk_size):
        end = min(start + chunk_size, len(tokens))
        chunk = np.asarray(tokens[start:end])
        if np.any(chunk >= MODEL_SPEC.vocab_size):
            raise ValueError(f"token IDs must be in 0..{MODEL_SPEC.vocab_size - 1}")
        bos_positions = np.flatnonzero(chunk == MODEL_SPEC.bos_token_id) + start
        eos_positions = np.flatnonzero(chunk == MODEL_SPEC.eos_token_id) + start
        if not np.array_equal(
            bos_positions,
            expected_bos[bos_cursor : bos_cursor + len(bos_positions)],
        ):
            raise ValueError("BOS tokens must occur exactly at story starts")
        if not np.array_equal(
            eos_positions,
            expected_eos[eos_cursor : eos_cursor + len(eos_positions)],
        ):
            raise ValueError("EOS tokens must occur exactly at story ends")
        bos_cursor += len(bos_positions)
        eos_cursor += len(eos_positions)
    if bos_cursor != len(expected_bos):
        raise ValueError("every story must start with BOS")
    if eos_cursor != len(expected_eos):
        raise ValueError("every story must end with EOS")


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def sample_batch(
    stream: TokenStream,
    *,
    batch_size: int,
    generator: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    if batch_size <= 0:
        raise ValueError("batch_size must be positive")
    selected = generator.integers(0, len(stream.windows), size=batch_size)
    return batch_from_window_indices(stream, window_indices=selected)


def batch_from_window_indices(
    stream: TokenStream,
    *,
    window_indices: np.ndarray,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    if not isinstance(window_indices, np.ndarray):
        raise TypeError("window_indices must be a NumPy array")
    if window_indices.ndim != 1:
        raise ValueError("window_indices must be one-dimensional")
    if window_indices.dtype.kind not in ("i", "u"):
        raise TypeError("window_indices must have an integer dtype")
    if len(window_indices) == 0:
        raise ValueError("window_indices must not be empty")
    if np.any(window_indices < 0) or np.any(window_indices >= len(stream.windows)):
        raise ValueError(f"window_indices must be in 0..{len(stream.windows) - 1}")
    batch_size = len(window_indices)
    sequence_length = MODEL_SPEC.context_length
    inputs = np.full(
        (batch_size, sequence_length), MODEL_SPEC.eos_token_id, dtype=np.int64
    )
    targets = np.full_like(inputs, MODEL_SPEC.eos_token_id)
    loss_mask = np.zeros_like(inputs, dtype=np.bool_)
    for row, window_index in enumerate(window_indices):
        window = stream.windows[window_index]
        start = int(window["start"])
        loss_start = int(window["loss_start"])
        loss_end = int(window["loss_end"])
        available = loss_end
        inputs[row, :available] = stream.tokens[start : start + available]
        targets[row, :available] = stream.tokens[start + 1 : start + available + 1]
        loss_mask[row, loss_start:loss_end] = True
    return inputs, targets, loss_mask
