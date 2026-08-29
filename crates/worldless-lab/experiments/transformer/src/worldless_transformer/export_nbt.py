"""Binary NBT exporter for the Worldless transformer command storage."""

from __future__ import annotations

import re
import struct
import zlib
from collections.abc import Iterable, Sequence
from pathlib import Path

from .artifact import ArtifactValidationError, ModelArtifact
from .spec import MODEL_SPEC, expected_weight_shapes

# This is the command-storage data version accepted by the target Minecraft
# version and by worldless::command_storage_file.
COMMAND_STORAGE_DATA_VERSION = 5015

_TAG_END = 0
_TAG_INT = 3
_TAG_BYTE_ARRAY = 7
_TAG_STRING = 8
_TAG_COMPOUND = 10
_TAG_INT_ARRAY = 11

_RESOURCE_PATH = re.compile(r"[a-z0-9._/-]+", re.ASCII)


class NbtExportError(ValueError):
    """The selected storage path or an encoded ABI value is invalid."""


def encode_command_storage(
    artifact: ModelArtifact,
    *,
    storage_path: str = "model",
    compressed: bool = True,
) -> bytes:
    """Encode one model as a Java Edition command-storage ``.dat`` file.

    The namespace is deliberately not embedded in the file.  Minecraft derives
    it from the filename and Worldless receives it alongside the filename.
    ``storage_path`` is the identifier path under that namespace.
    """

    if not isinstance(artifact, ModelArtifact):
        raise TypeError("artifact must be a ModelArtifact")
    if not isinstance(compressed, bool):
        raise NbtExportError("compressed must be a boolean")
    artifact.validate()
    _validate_storage_path(storage_path)

    bundle = _compound_payload(
        (
            ("abi", _TAG_COMPOUND, _abi_payload(artifact)),
            ("weights", _TAG_COMPOUND, _weights_payload(artifact)),
            ("biases", _TAG_COMPOUND, _compound_payload(())),
            ("shifts", _TAG_COMPOUND, _shifts_payload(artifact)),
        )
    )
    contents = _compound_payload(((storage_path, _TAG_COMPOUND, bundle),))
    data = _compound_payload((("contents", _TAG_COMPOUND, contents),))
    root = _named_tag(
        _TAG_COMPOUND,
        "",
        _compound_payload(
            (
                ("DataVersion", _TAG_INT, _int_payload(COMMAND_STORAGE_DATA_VERSION)),
                ("data", _TAG_COMPOUND, data),
            )
        ),
    )
    return _deterministic_gzip(root) if compressed else root


def write_command_storage(
    path: str | Path,
    artifact: ModelArtifact,
    *,
    storage_path: str = "model",
    compressed: bool = True,
) -> None:
    """Write a new file without replacing an existing artifact."""

    output = Path(path)
    encoded = encode_command_storage(
        artifact,
        storage_path=storage_path,
        compressed=compressed,
    )
    with output.open("xb") as file:
        written = file.write(encoded)
        if written != len(encoded):
            raise OSError(
                f"short write for {output}: wrote {written} of {len(encoded)} bytes"
            )


def _abi_payload(artifact: ModelArtifact) -> bytes:
    spec = MODEL_SPEC
    return _compound_payload(
        (
            ("schema", _TAG_INT, _int_payload(spec.schema_version)),
            (
                "architecture_id",
                _TAG_STRING,
                _string_payload(artifact.architecture_id, "architecture_id"),
            ),
            (
                "tokenizer_id",
                _TAG_INT_ARRAY,
                _int_array_payload(artifact.tokenizer_id, "tokenizer_id"),
            ),
            (
                "tokenizer_kind",
                _TAG_STRING,
                _string_payload(spec.tokenizer_kind, "tokenizer_kind"),
            ),
            ("vocab_size", _TAG_INT, _int_payload(spec.vocab_size)),
            ("bos_id", _TAG_INT, _int_payload(spec.bos_token_id)),
            ("eos_id", _TAG_INT, _int_payload(spec.eos_token_id)),
        )
    )


def _weights_payload(artifact: ModelArtifact) -> bytes:
    return _compound_payload(
        (
            name,
            _TAG_BYTE_ARRAY,
            _byte_array_payload(artifact.weights[name], name),
        )
        for name in expected_weight_shapes()
    )


def _shifts_payload(artifact: ModelArtifact) -> bytes:
    return _compound_payload(
        (
            name,
            _TAG_INT_ARRAY,
            _int_array_payload(artifact.shifts[name], f"shifts[{name!r}]"),
        )
        for name in expected_weight_shapes()
    )


def _compound_payload(entries: Iterable[tuple[str, int, bytes]]) -> bytes:
    output = bytearray()
    seen: set[str] = set()
    for name, tag_type, payload in entries:
        if name in seen:
            raise NbtExportError(f"duplicate NBT compound field {name!r}")
        seen.add(name)
        output.extend(_named_tag(tag_type, name, payload))
    output.append(_TAG_END)
    return bytes(output)


def _named_tag(tag_type: int, name: str, payload: bytes) -> bytes:
    if tag_type == _TAG_END:
        raise NbtExportError("TAG_End cannot have a name")
    return bytes((tag_type,)) + _modified_utf_payload(name, "NBT field name") + payload


def _int_payload(value: int) -> bytes:
    if isinstance(value, bool) or not isinstance(value, int):
        raise NbtExportError(f"NBT int must be an integer, got {value!r}")
    try:
        return struct.pack(">i", value)
    except struct.error as error:
        raise NbtExportError(f"NBT int is outside signed int32: {value!r}") from error


def _byte_array_payload(value: bytes, field: str) -> bytes:
    if not isinstance(value, bytes):
        raise ArtifactValidationError(f"weight {field!r} must be encoded bytes")
    return _array_length(len(value), field) + value


def _int_array_payload(value: Sequence[int], field: str) -> bytes:
    output = bytearray(_array_length(len(value), field))
    for index, item in enumerate(value):
        try:
            output.extend(_int_payload(item))
        except NbtExportError as error:
            raise NbtExportError(f"{field}[{index}]: {error}") from error
    return bytes(output)


def _array_length(length: int, field: str) -> bytes:
    if not 0 <= length < (1 << 31):
        raise NbtExportError(
            f"{field} array length is outside the NBT signed-int32 range: {length}"
        )
    return struct.pack(">i", length)


def _string_payload(value: str, field: str) -> bytes:
    return _modified_utf_payload(value, field)


def _modified_utf_payload(value: str, field: str) -> bytes:
    if not isinstance(value, str):
        raise NbtExportError(f"{field} must be a string")
    # Every string in this ABI is deliberately ASCII, which is byte-identical
    # under UTF-8 and Java's modified UTF-8 and avoids multiple encodings.
    if not value.isascii() or "\x00" in value:
        raise NbtExportError(f"{field} must contain non-NUL ASCII only")
    encoded = value.encode("ascii")
    if len(encoded) > 0xFFFF:
        raise NbtExportError(f"{field} exceeds the NBT string length limit")
    return struct.pack(">H", len(encoded)) + encoded


def _validate_storage_path(value: str) -> None:
    if not isinstance(value, str) or _RESOURCE_PATH.fullmatch(value) is None:
        raise NbtExportError(
            "storage_path must be a non-empty Minecraft resource path using "
            "[a-z0-9._/-]"
        )


def _deterministic_gzip(value: bytes) -> bytes:
    compressor = zlib.compressobj(
        level=9,
        method=zlib.DEFLATED,
        wbits=-zlib.MAX_WBITS,
    )
    payload = compressor.compress(value) + compressor.flush()
    # MTIME=0, XFL=maximum compression, and OS=unknown make the header stable
    # across platforms.  The trailer is little-endian per RFC 1952.
    header = b"\x1f\x8b\x08\x00\x00\x00\x00\x00\x02\xff"
    trailer = struct.pack("<II", zlib.crc32(value), len(value) & 0xFFFF_FFFF)
    return header + payload + trailer
