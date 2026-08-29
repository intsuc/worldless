from __future__ import annotations

import gzip
import hashlib
import math
import struct
from pathlib import Path
from typing import Any

import pytest

from worldless_transformer.artifact import ModelArtifact
from worldless_transformer.export_nbt import (
    COMMAND_STORAGE_DATA_VERSION,
    NbtExportError,
    encode_command_storage,
    write_command_storage,
)
from worldless_transformer.spec import MODEL_SPEC, expected_weight_shapes


def _artifact() -> ModelArtifact:
    shapes = expected_weight_shapes()
    weights = {name: bytes(math.prod(shape)) for name, shape in shapes.items()}
    embedding = bytearray(weights["token_embedding.weight"])
    embedding[:4] = b"\x81\xff\x00\x7f"
    weights["token_embedding.weight"] = bytes(embedding)
    shifts = {name: index % 31 for index, name in enumerate(shapes)}
    shifts["token_embedding.weight"] = 0
    return ModelArtifact.create(
        tokenizer_id=hashlib.sha256(b"canonical tokenizer json").digest(),
        weights=weights,
        shifts=shifts,
    )


class _NbtReader:
    def __init__(self, value: bytes) -> None:
        self.value = value
        self.offset = 0

    def read_root(self) -> dict[str, Any]:
        assert self._u8() == 10
        assert self._string() == ""
        result = self._compound()
        assert self.offset == len(self.value)
        return result

    def _payload(self, tag_type: int) -> Any:
        if tag_type == 3:
            return self._i32()
        if tag_type == 7:
            length = self._i32()
            assert length >= 0
            return self._take(length)
        if tag_type == 8:
            return self._string()
        if tag_type == 10:
            return self._compound()
        if tag_type == 11:
            length = self._i32()
            assert length >= 0
            return tuple(self._i32() for _ in range(length))
        raise AssertionError(f"unexpected tag type {tag_type}")

    def _compound(self) -> dict[str, Any]:
        result = {}
        while (tag_type := self._u8()) != 0:
            name = self._string()
            assert name not in result
            result[name] = self._payload(tag_type)
        return result

    def _u8(self) -> int:
        return self._take(1)[0]

    def _i32(self) -> int:
        return struct.unpack(">i", self._take(4))[0]

    def _string(self) -> str:
        length = struct.unpack(">H", self._take(2))[0]
        return self._take(length).decode("ascii")

    def _take(self, length: int) -> bytes:
        end = self.offset + length
        assert end <= len(self.value)
        result = self.value[self.offset : end]
        self.offset = end
        return result


def _decode(value: bytes) -> dict[str, Any]:
    if value.startswith(b"\x1f\x8b"):
        value = gzip.decompress(value)
    return _NbtReader(value).read_root()


def test_export_matches_worldless_command_storage_envelope_and_abi() -> None:
    artifact = _artifact()

    root = _decode(encode_command_storage(artifact))

    assert set(root) == {"DataVersion", "data"}
    assert root["DataVersion"] == COMMAND_STORAGE_DATA_VERSION == 5015
    assert set(root["data"]) == {"contents"}
    assert set(root["data"]["contents"]) == {"model"}

    bundle = root["data"]["contents"]["model"]
    assert set(bundle) == {"abi", "weights", "biases", "shifts"}
    assert bundle["biases"] == {}
    assert set(bundle["weights"]) == set(expected_weight_shapes())
    assert set(bundle["shifts"]) == set(expected_weight_shapes())
    assert struct.unpack(">4b", bundle["weights"]["token_embedding.weight"][:4]) == (
        -127,
        -1,
        0,
        127,
    )

    abi = bundle["abi"]
    assert set(abi) == {
        "schema",
        "architecture_id",
        "tokenizer_id",
        "tokenizer_kind",
        "vocab_size",
        "bos_id",
        "eos_id",
    }
    assert abi == {
        "schema": MODEL_SPEC.schema_version,
        "architecture_id": MODEL_SPEC.architecture_id,
        "tokenizer_id": artifact.tokenizer_id,
        "tokenizer_kind": MODEL_SPEC.tokenizer_kind,
        "vocab_size": 512,
        "bos_id": 510,
        "eos_id": 511,
    }
    assert bundle["shifts"]["blocks.0.attention.q_proj.weight"] == (1,)


def test_gzip_and_uncompressed_encodings_are_deterministic() -> None:
    artifact = _artifact()

    first = encode_command_storage(artifact)
    second = encode_command_storage(artifact)
    raw = encode_command_storage(artifact, compressed=False)

    assert first == second
    assert first[:10] == b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff"
    assert gzip.decompress(first) == raw
    assert raw[:3] == b"\x0a\x00\x00"


@pytest.mark.parametrize("storage_path", ["", "Model", "namespace:model", "snowman_☃"])
def test_export_rejects_invalid_storage_paths(storage_path: str) -> None:
    with pytest.raises(NbtExportError, match="storage_path"):
        encode_command_storage(_artifact(), storage_path=storage_path)


def test_export_uses_requested_storage_path_and_writes_file(tmp_path: Path) -> None:
    artifact = _artifact()
    output = tmp_path / "weights.dat"

    write_command_storage(output, artifact, storage_path="models/tiny_stories")

    encoded = output.read_bytes()
    contents = _decode(encoded)["data"]["contents"]
    assert set(contents) == {"models/tiny_stories"}
    assert encoded == encode_command_storage(
        artifact, storage_path="models/tiny_stories"
    )
    with pytest.raises(FileExistsError):
        write_command_storage(output, artifact)
