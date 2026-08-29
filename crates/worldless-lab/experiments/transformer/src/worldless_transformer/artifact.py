"""Validated, runtime-ready quantized model artifacts.

The architecture and tensor layout live in :mod:`worldless_transformer.spec`.
This module owns only the validated binary artifact contract.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from types import MappingProxyType

from .spec import (
    ModelSpec,
    expected_weight_shapes,
    spec_for_architecture_id,
    zero_shift_weight_names,
)


class ArtifactValidationError(ValueError):
    """A model artifact does not satisfy the fixed Worldless runtime ABI."""


def _digest_to_int_array(digest: bytes | str) -> tuple[int, ...]:
    if isinstance(digest, bytes):
        if len(digest) != 32:
            raise ArtifactValidationError(
                f"tokenizer_id SHA-256 digest must contain 32 bytes, got {len(digest)}"
            )
        tokenizer_id = digest.hex()
    elif isinstance(digest, str):
        tokenizer_id = digest
    else:
        raise ArtifactValidationError(
            "tokenizer_id must be a SHA-256 bytes or hex value"
        )

    from .tokenizer import tokenizer_id_to_int_array

    try:
        return tokenizer_id_to_int_array(tokenizer_id)
    except ValueError as error:
        raise ArtifactValidationError(str(error)) from error


@dataclass(frozen=True, slots=True)
class ModelArtifact:
    """A complete quantized model with no implicit runtime defaults.

    ``weights`` contains symmetric ``[-127, 127]`` int8 tensors in
    C-contiguous row-major order.
    The mathematical architecture is bias-free, so ``biases`` is required to
    be empty.  Every weight tensor has one explicit signed int32 requantization
    shift exponent, avoiding a missing-value-means-zero convention in the data
    pack.  Requantization uses round-half-away-from-zero followed by a power-of-
    two right shift; the exponent is therefore constrained to ``0..30``.
    """

    architecture_id: str
    tokenizer_id: tuple[int, ...]
    weights: Mapping[str, bytes]
    biases: Mapping[str, tuple[int, ...]]
    shifts: Mapping[str, tuple[int, ...]]

    @classmethod
    def create(
        cls,
        *,
        spec: ModelSpec,
        tokenizer_id: bytes | str | Sequence[int],
        weights: Mapping[str, object],
        shifts: Mapping[str, Sequence[int] | int],
        biases: Mapping[str, Sequence[int]] | None = None,
    ) -> ModelArtifact:
        """Validate and freeze values supplied by a quantization pipeline."""

        try:
            selected_spec = spec_for_architecture_id(spec.architecture_id)
        except (AttributeError, ValueError) as error:
            raise ArtifactValidationError(
                "spec must identify a known architecture"
            ) from error
        if spec != selected_spec:
            raise ArtifactValidationError(
                "spec must exactly match a known architecture"
            )

        if isinstance(tokenizer_id, (bytes, str)):
            normalized_tokenizer_id = _digest_to_int_array(tokenizer_id)
        else:
            normalized_tokenizer_id = _int32_array(
                tokenizer_id, "tokenizer_id", expected_length=8
            )

        shapes = MappingProxyType(expected_weight_shapes(selected_spec))
        _require_exact_keys(weights, shapes, "weights")
        normalized_weights = {
            name: _int8_tensor_bytes(weights[name], shape, f"weights[{name!r}]")
            for name, shape in shapes.items()
        }

        if biases is not None and not isinstance(biases, Mapping):
            raise ArtifactValidationError("biases must be a mapping")
        normalized_biases = {} if biases is None else dict(biases)
        if normalized_biases:
            names = ", ".join(sorted(map(repr, normalized_biases)))
            raise ArtifactValidationError(
                f"biases must be empty for the bias-free architecture, got {names}"
            )

        _require_exact_keys(shifts, shapes, "shifts")
        normalized_shifts: dict[str, tuple[int, ...]] = {}
        shift_min, shift_max = _requant_shift_bounds()
        for name in shapes:
            value = shifts[name]
            values = (value,) if _is_plain_int(value) else value
            normalized = _int32_array(values, f"shifts[{name!r}]", expected_length=1)
            if not shift_min <= normalized[0] <= shift_max:
                raise ArtifactValidationError(
                    f"shifts[{name!r}][0] must be in {shift_min}..{shift_max}, "
                    f"got {normalized[0]}"
                )
            normalized_shifts[name] = normalized

        for name in zero_shift_weight_names(selected_spec):
            shift = normalized_shifts[name][0]
            if shift != 0:
                raise ArtifactValidationError(
                    f"shifts[{name!r}][0] must be 0, got {shift}"
                )

        return cls(
            architecture_id=selected_spec.architecture_id,
            tokenizer_id=normalized_tokenizer_id,
            weights=MappingProxyType(normalized_weights),
            biases=MappingProxyType({}),
            shifts=MappingProxyType(normalized_shifts),
        )

    def __post_init__(self) -> None:
        self.validate()
        object.__setattr__(self, "weights", MappingProxyType(dict(self.weights)))
        object.__setattr__(self, "biases", MappingProxyType({}))
        object.__setattr__(
            self,
            "shifts",
            MappingProxyType(
                {name: tuple(value) for name, value in self.shifts.items()}
            ),
        )

    def validate(self) -> None:
        """Recheck the complete artifact, including externally supplied mappings."""

        try:
            spec = spec_for_architecture_id(self.architecture_id)
        except ValueError as error:
            raise ArtifactValidationError(
                "architecture_id must identify a known architecture"
            ) from error
        if not isinstance(self.tokenizer_id, tuple):
            raise ArtifactValidationError(
                "construct ModelArtifact with ModelArtifact.create()"
            )
        _int32_array(self.tokenizer_id, "tokenizer_id", expected_length=8)
        shapes = MappingProxyType(expected_weight_shapes(spec))
        _require_exact_keys(self.weights, shapes, "weights")
        for name, shape in shapes.items():
            value = self.weights[name]
            if not isinstance(value, bytes) or len(value) != _element_count(shape):
                raise ArtifactValidationError(
                    "construct ModelArtifact with ModelArtifact.create()"
                )
            _validate_int8_bytes(value, f"weights[{name!r}]")
        if not isinstance(self.biases, Mapping):
            raise ArtifactValidationError("biases must be a mapping")
        if self.biases:
            raise ArtifactValidationError("biases must be empty for this architecture")
        _require_exact_keys(self.shifts, shapes, "shifts")
        shift_min, shift_max = _requant_shift_bounds()
        for name in shapes:
            normalized = _int32_array(
                self.shifts[name], f"shifts[{name!r}]", expected_length=1
            )
            if not shift_min <= normalized[0] <= shift_max:
                raise ArtifactValidationError(
                    f"shifts[{name!r}][0] must be in {shift_min}..{shift_max}, "
                    f"got {normalized[0]}"
                )
        for name in zero_shift_weight_names(spec):
            if self.shifts[name][0] != 0:
                raise ArtifactValidationError(f"shifts[{name!r}][0] must be 0")


def _require_exact_keys(
    values: Mapping[str, object],
    expected: Mapping[str, object],
    field: str,
) -> None:
    if not isinstance(values, Mapping):
        raise ArtifactValidationError(f"{field} must be a mapping")
    invalid_keys = [key for key in values if not isinstance(key, str)]
    if invalid_keys:
        rendered = ", ".join(map(repr, invalid_keys))
        raise ArtifactValidationError(f"{field} keys must be strings, got {rendered}")
    supplied = set(values)
    required = set(expected)
    missing = sorted(required - supplied)
    unknown = sorted(supplied - required)
    if missing or unknown:
        details = []
        if missing:
            details.append("missing " + ", ".join(map(repr, missing)))
        if unknown:
            details.append("unknown " + ", ".join(map(repr, unknown)))
        raise ArtifactValidationError(f"invalid {field} keys: {'; '.join(details)}")


def _int8_tensor_bytes(value: object, shape: tuple[int, ...], field: str) -> bytes:
    count = _element_count(shape)
    if isinstance(value, bytes):
        if len(value) != count:
            raise ArtifactValidationError(
                f"{field} must contain {count} int8 values for shape {shape}, "
                f"got {len(value)}"
            )
        _validate_int8_bytes(value, field)
        return value

    actual_shape = getattr(value, "shape", None)
    if actual_shape is not None:
        try:
            normalized_shape = tuple(int(dimension) for dimension in actual_shape)
        except (TypeError, ValueError) as error:
            raise ArtifactValidationError(f"{field} has an invalid shape") from error
        if normalized_shape != shape:
            raise ArtifactValidationError(
                f"{field} must have shape {shape}, got {normalized_shape}"
            )

        dtype = str(getattr(value, "dtype", ""))
        if dtype not in {"int8", "torch.int8"}:
            raise ArtifactValidationError(
                f"{field} must use signed int8 values, got dtype {dtype!r}"
            )
        candidate = value
        detach = getattr(candidate, "detach", None)
        if callable(detach):
            candidate = detach()
        cpu = getattr(candidate, "cpu", None)
        if callable(cpu):
            candidate = cpu()
        contiguous = getattr(candidate, "contiguous", None)
        if callable(contiguous):
            candidate = contiguous()
        numpy = getattr(candidate, "numpy", None)
        if callable(numpy):
            candidate = numpy()
        tobytes = getattr(candidate, "tobytes", None)
        if not callable(tobytes):
            raise ArtifactValidationError(
                f"{field} signed int8 tensor cannot be serialized as contiguous bytes"
            )
        try:
            raw = tobytes(order="C")
        except TypeError:
            raw = tobytes()
        if not isinstance(raw, bytes) or len(raw) != count:
            raise ArtifactValidationError(
                f"{field} did not serialize to exactly {count} bytes"
            )
        _validate_int8_bytes(raw, field)
        return raw

    if isinstance(value, Sequence) and not isinstance(value, (str, bytearray)):
        if len(value) != count:
            raise ArtifactValidationError(
                f"{field} must contain {count} int8 values for shape {shape}, "
                f"got {len(value)}"
            )
        output = bytearray(count)
        int8_min, int8_max = _int8_bounds()
        for index, item in enumerate(value):
            if not _is_plain_int(item) or not int8_min <= item <= int8_max:
                raise ArtifactValidationError(
                    f"{field}[{index}] must be an integer in "
                    f"{int8_min}..{int8_max}, got {item!r}"
                )
            output[index] = item & 0xFF
        return bytes(output)

    raise ArtifactValidationError(
        f"{field} must be bytes, a flat signed-int8 sequence, or an int8 tensor"
    )


def _int32_array(
    value: Sequence[int], field: str, *, expected_length: int
) -> tuple[int, ...]:
    if isinstance(value, (str, bytes, bytearray)) or not isinstance(value, Sequence):
        raise ArtifactValidationError(f"{field} must be an integer sequence")
    if len(value) != expected_length:
        raise ArtifactValidationError(
            f"{field} must contain {expected_length} integers, got {len(value)}"
        )
    normalized = []
    int32_min, int32_max = _int32_bounds()
    for index, item in enumerate(value):
        if not _is_plain_int(item) or not int32_min <= item <= int32_max:
            raise ArtifactValidationError(
                f"{field}[{index}] must be a signed int32 integer, got {item!r}"
            )
        normalized.append(item)
    return tuple(normalized)


def _element_count(shape: tuple[int, ...]) -> int:
    count = 1
    for dimension in shape:
        if not _is_plain_int(dimension) or dimension <= 0:
            raise RuntimeError(f"model spec produced invalid tensor shape {shape!r}")
        count *= dimension
    return count


def _validate_int8_bytes(value: bytes, field: str) -> None:
    int8_min, int8_max = _int8_bounds()
    for index, encoded in enumerate(value):
        decoded = encoded if encoded < 128 else encoded - 256
        if not int8_min <= decoded <= int8_max:
            raise ArtifactValidationError(
                f"{field}[{index}] must be in {int8_min}..{int8_max}, got {decoded}"
            )


def _int8_bounds() -> tuple[int, int]:
    from .spec import INT8_MAX, INT8_MIN

    return INT8_MIN, INT8_MAX


def _requant_shift_bounds() -> tuple[int, int]:
    from .spec import REQUANT_SHIFT_MAX, REQUANT_SHIFT_MIN

    return REQUANT_SHIFT_MIN, REQUANT_SHIFT_MAX


def _int32_bounds() -> tuple[int, int]:
    from .spec import INT32_MAX, INT32_MIN

    return INT32_MIN, INT32_MAX


def _is_plain_int(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)
