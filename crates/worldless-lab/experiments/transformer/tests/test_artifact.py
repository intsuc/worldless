from __future__ import annotations

import hashlib
import math
import struct

import pytest

from worldless_transformer.artifact import (
    ArtifactValidationError,
    ModelArtifact,
)
from worldless_transformer.model import Transformer
from worldless_transformer.spec import ARCHITECTURE_ID, expected_weight_shapes


def _valid_values() -> tuple[dict[str, bytes], dict[str, int]]:
    shapes = expected_weight_shapes()
    weights = {name: bytes(math.prod(shape)) for name, shape in shapes.items()}
    shifts = {name: 0 for name in shapes}
    return weights, shifts


def _artifact() -> ModelArtifact:
    weights, shifts = _valid_values()
    return ModelArtifact.create(
        tokenizer_id=hashlib.sha256(b"tokenizer").digest(),
        weights=weights,
        shifts=shifts,
    )


def test_layout_matches_the_fixed_bias_free_model() -> None:
    shapes = expected_weight_shapes()

    assert list(shapes)[:2] == [
        "token_embedding.weight",
        "blocks.0.attention.q_proj.weight",
    ]
    assert shapes["token_embedding.weight"] == (512, 96)
    assert shapes["blocks.3.attention.k_proj.weight"] == (16, 96)
    assert shapes["blocks.3.ffn.down_proj.weight"] == (96, 192)
    assert len(shapes) == 25
    assert sum(math.prod(shape) for shape in shapes.values()) == 282_624


def test_create_preserves_tokenizer_digest_as_signed_big_endian_words() -> None:
    digest = bytes.fromhex(
        "000000017fffffff80000000ffffffff0123456789abcdeffedcba9876543210"
    )
    weights, shifts = _valid_values()

    artifact = ModelArtifact.create(
        tokenizer_id=digest.hex(), weights=weights, shifts=shifts
    )

    assert artifact.tokenizer_id[:4] == (
        1,
        2_147_483_647,
        -2_147_483_648,
        -1,
    )
    assert artifact.tokenizer_id == struct.unpack(">8i", digest)


def test_create_validates_and_freezes_complete_artifact() -> None:
    artifact = _artifact()

    assert artifact.architecture_id == ARCHITECTURE_ID
    assert len(artifact.tokenizer_id) == 8
    assert not artifact.biases
    assert artifact.shifts["token_embedding.weight"] == (0,)
    with pytest.raises(TypeError):
        artifact.weights["token_embedding.weight"] = b""  # type: ignore[index]


def test_create_accepts_the_models_quantized_runtime_state() -> None:
    state = Transformer().runtime_state()

    artifact = ModelArtifact.create(
        tokenizer_id=bytes(32), weights=state.weights, shifts=state.shifts
    )

    assert len(artifact.weights["blocks.0.ffn.up_proj.weight"]) == 192 * 96


@pytest.mark.parametrize("field", ["weights", "shifts"])
def test_create_rejects_missing_and_unknown_tensor_keys(field: str) -> None:
    weights, shifts = _valid_values()
    selected = weights if field == "weights" else shifts
    selected.pop("blocks.0.attention.q_proj.weight")
    selected["unknown.weight"] = b"" if field == "weights" else 0

    with pytest.raises(ArtifactValidationError, match=rf"invalid {field} keys"):
        ModelArtifact.create(
            tokenizer_id=bytes(32),
            weights=weights,
            shifts=shifts,
        )


def test_create_rejects_wrong_weight_shape_and_non_int8_sequence() -> None:
    weights, shifts = _valid_values()
    weights["token_embedding.weight"] = bytes(511 * 96)
    with pytest.raises(ArtifactValidationError, match="must contain 49152 int8"):
        ModelArtifact.create(tokenizer_id=bytes(32), weights=weights, shifts=shifts)

    weights, shifts = _valid_values()
    weights["blocks.0.attention.k_proj.weight"] = [0.0] * (16 * 96)  # type: ignore[assignment]
    with pytest.raises(
        ArtifactValidationError, match="must be an integer in -127..127"
    ):
        ModelArtifact.create(tokenizer_id=bytes(32), weights=weights, shifts=shifts)

    weights, shifts = _valid_values()
    invalid = bytearray(weights["blocks.0.attention.k_proj.weight"])
    invalid[7] = 0x80
    weights["blocks.0.attention.k_proj.weight"] = bytes(invalid)
    with pytest.raises(ArtifactValidationError, match=r"must be in -127\.\.127"):
        ModelArtifact.create(tokenizer_id=bytes(32), weights=weights, shifts=shifts)


def test_create_rejects_biases_and_invalid_shift_exponents() -> None:
    weights, shifts = _valid_values()
    with pytest.raises(ArtifactValidationError, match="biases must be empty"):
        ModelArtifact.create(
            tokenizer_id=bytes(32),
            weights=weights,
            shifts=shifts,
            biases={"blocks.0.attention.q_proj.weight": [0]},
        )

    shifts["blocks.0.attention.q_proj.weight"] = 31
    with pytest.raises(ArtifactValidationError, match="must be in 0..30"):
        ModelArtifact.create(tokenizer_id=bytes(32), weights=weights, shifts=shifts)

    shifts["blocks.0.attention.q_proj.weight"] = 0
    shifts["token_embedding.weight"] = 1
    with pytest.raises(ArtifactValidationError, match="must be 0"):
        ModelArtifact.create(tokenizer_id=bytes(32), weights=weights, shifts=shifts)


def test_create_rejects_wrong_architecture_and_tokenizer_ids() -> None:
    weights, shifts = _valid_values()
    with pytest.raises(ArtifactValidationError, match="architecture_id does not match"):
        ModelArtifact.create(
            architecture_id="other",
            tokenizer_id=bytes(32),
            weights=weights,
            shifts=shifts,
        )

    with pytest.raises(ArtifactValidationError, match="must contain 32 bytes"):
        ModelArtifact.create(tokenizer_id=bytes(31), weights=weights, shifts=shifts)
    with pytest.raises(ArtifactValidationError, match="lowercase hexadecimal"):
        ModelArtifact.create(tokenizer_id="A" * 64, weights=weights, shifts=shifts)
