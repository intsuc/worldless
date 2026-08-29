from __future__ import annotations

import hashlib
import json
import re
import struct
from collections import Counter
from collections.abc import Iterable, Iterator, Sequence
from dataclasses import dataclass
from itertools import pairwise
from pathlib import Path
from typing import Final

from .spec import DATA_ABI_ID, DATA_SCHEMA_VERSION, DATA_SPEC

TOKENIZER_KIND: Final = "greedy_string_piece"
_ARTIFACT_KEYS: Final = {
    "data_abi_id",
    "bos_token_id",
    "eos_token_id",
    "kind",
    "pieces",
    "schema_version",
    "tokenizer_id",
    "vocab_size",
}
_ATOM_PATTERN: Final = re.compile(r"\s+\S+|\S+|\s+")


class UnsupportedTextError(ValueError):
    pass


@dataclass(slots=True)
class _TrieNode:
    children: dict[str, _TrieNode]
    token_id: int | None = None


def _validate_scalar_text(text: str, *, role: str) -> None:
    for index, scalar in enumerate(text):
        value = ord(scalar)
        if 0xD800 <= value <= 0xDFFF:
            raise ValueError(
                f"{role} contains a lone UTF-16 surrogate at Python index {index}"
            )


def java_utf16_length(text: str) -> int:
    _validate_scalar_text(text, role="text")
    return len(text.encode("utf-16-be")) // 2


def _piece_sort_key(piece: str) -> bytes:
    return piece.encode("utf-16-be")


def _canonical_bytes(payload: dict[str, object]) -> bytes:
    return json.dumps(
        payload,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def tokenizer_id_to_int_array(tokenizer_id: str) -> tuple[int, ...]:
    if not isinstance(tokenizer_id, str) or len(tokenizer_id) != 64:
        raise ValueError("tokenizer_id must be 64 lowercase hexadecimal characters")
    if tokenizer_id != tokenizer_id.lower():
        raise ValueError("tokenizer_id must use lowercase hexadecimal")
    try:
        digest = bytes.fromhex(tokenizer_id)
    except ValueError as error:
        raise ValueError(
            "tokenizer_id must be 64 lowercase hexadecimal characters"
        ) from error
    return struct.unpack(">8i", digest)


class GreedyStringPieceTokenizer:
    """Lossless longest-prefix tokenizer over Unicode scalar boundaries.

    Minecraft indexes StringTag values in Java UTF-16 code units. Pieces never
    split a Unicode scalar, so longest scalar-prefix and longest UTF-16-prefix
    select the same piece; ``java_utf16_length`` exposes the runtime cursor
    increment explicitly for generated data-pack code.
    """

    def __init__(self, pieces: Sequence[str]) -> None:
        expected = DATA_SPEC.regular_piece_count
        if len(pieces) != expected:
            raise ValueError(f"expected exactly {expected} regular pieces")
        if len(set(pieces)) != len(pieces):
            raise ValueError("regular pieces must be unique")

        root = _TrieNode({})
        scalar_pieces: set[str] = set()
        used_scalars: set[str] = set()
        checked: list[str] = []
        for token_id, piece in enumerate(pieces):
            if not isinstance(piece, str) or not piece:
                raise ValueError(f"piece {token_id} must be a non-empty string")
            _validate_scalar_text(piece, role=f"piece {token_id}")
            used_scalars.update(piece)
            if len(piece) == 1:
                scalar_pieces.add(piece)
            node = root
            for scalar in piece:
                node = node.children.setdefault(scalar, _TrieNode({}))
            node.token_id = token_id
            checked.append(piece)

        missing_base_pieces = sorted(used_scalars - scalar_pieces, key=_piece_sort_key)
        if missing_base_pieces:
            formatted = ", ".join(
                f"U+{ord(scalar):04X}" for scalar in missing_base_pieces
            )
            raise ValueError(
                f"pieces are missing single-scalar base tokens: {formatted}"
            )

        self._pieces = tuple(checked)
        self._root = root

    @property
    def pieces(self) -> tuple[str, ...]:
        return self._pieces

    @property
    def tokenizer_id(self) -> str:
        return hashlib.sha256(_canonical_bytes(self._payload())).hexdigest()

    @property
    def tokenizer_id_bytes(self) -> bytes:
        return bytes.fromhex(self.tokenizer_id)

    @property
    def tokenizer_id_int_array(self) -> tuple[int, ...]:
        return tokenizer_id_to_int_array(self.tokenizer_id)

    def _payload(self) -> dict[str, object]:
        return {
            "data_abi_id": DATA_ABI_ID,
            "bos_token_id": DATA_SPEC.bos_token_id,
            "eos_token_id": DATA_SPEC.eos_token_id,
            "kind": TOKENIZER_KIND,
            "pieces": list(self._pieces),
            "schema_version": DATA_SCHEMA_VERSION,
            "vocab_size": DATA_SPEC.vocab_size,
        }

    def to_dict(self) -> dict[str, object]:
        payload = self._payload()
        payload["tokenizer_id"] = self.tokenizer_id
        return payload

    def save(self, path: str | Path) -> None:
        target = Path(path)
        if target.exists():
            raise FileExistsError(f"refusing to replace tokenizer: {target}")
        target.parent.mkdir(parents=True, exist_ok=True)
        encoded = json.dumps(
            self.to_dict(),
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            indent=2,
        )
        with target.open("x", encoding="utf-8") as output:
            output.write(encoded + "\n")

    @classmethod
    def load(cls, path: str | Path) -> GreedyStringPieceTokenizer:
        source = Path(path)
        value = json.loads(source.read_text(encoding="utf-8"))
        if not isinstance(value, dict):
            raise TypeError("tokenizer artifact must be a JSON object")
        actual_keys = set(value)
        if actual_keys != _ARTIFACT_KEYS:
            missing = sorted(_ARTIFACT_KEYS - actual_keys)
            unknown = sorted(actual_keys - _ARTIFACT_KEYS)
            raise ValueError(
                f"invalid tokenizer artifact fields: missing={missing}, unknown={unknown}"
            )
        expected_scalars = {
            "data_abi_id": DATA_ABI_ID,
            "bos_token_id": DATA_SPEC.bos_token_id,
            "eos_token_id": DATA_SPEC.eos_token_id,
            "kind": TOKENIZER_KIND,
            "schema_version": DATA_SCHEMA_VERSION,
            "vocab_size": DATA_SPEC.vocab_size,
        }
        for field, expected in expected_scalars.items():
            if value[field] != expected:
                raise ValueError(
                    f"tokenizer {field} must be {expected!r}, got {value[field]!r}"
                )
        pieces = value["pieces"]
        if not isinstance(pieces, list) or not all(
            isinstance(piece, str) for piece in pieces
        ):
            raise ValueError("tokenizer pieces must be a JSON string array")
        tokenizer = cls(pieces)
        if value["tokenizer_id"] != tokenizer.tokenizer_id:
            raise ValueError("tokenizer_id does not match tokenizer contents")
        return tokenizer

    def encode(self, text: str) -> list[int]:
        if not isinstance(text, str):
            raise TypeError("input must be a string")
        _validate_scalar_text(text, role="input")
        token_ids: list[int] = []
        position = 0
        while position < len(text):
            node = self._root
            cursor = position
            match_id: int | None = None
            match_end = position
            while cursor < len(text):
                child = node.children.get(text[cursor])
                if child is None:
                    break
                cursor += 1
                node = child
                if node.token_id is not None:
                    match_id = node.token_id
                    match_end = cursor
            if match_id is None:
                scalar = text[position]
                raise UnsupportedTextError(
                    f"unsupported Unicode scalar U+{ord(scalar):04X} at Python index "
                    f"{position}, UTF-16 index {java_utf16_length(text[:position])}"
                )
            token_ids.append(match_id)
            position = match_end
        return token_ids

    def encode_story(self, text: str) -> list[int]:
        return [DATA_SPEC.bos_token_id, *self.encode(text), DATA_SPEC.eos_token_id]

    def decode_text(self, token_ids: Iterable[int]) -> str:
        pieces: list[str] = []
        for position, token_id in enumerate(token_ids):
            if not isinstance(token_id, int) or isinstance(token_id, bool):
                raise TypeError(f"token ID at position {position} must be an integer")
            if not 0 <= token_id < DATA_SPEC.regular_piece_count:
                raise ValueError(
                    f"token ID at position {position} is not a regular piece: {token_id}"
                )
            pieces.append(self._pieces[token_id])
        return "".join(pieces)

    def decode_completion(self, token_ids: Iterable[int]) -> str:
        pieces: list[str] = []
        ended = False
        for position, token_id in enumerate(token_ids):
            if not isinstance(token_id, int) or isinstance(token_id, bool):
                raise TypeError(f"token ID at position {position} must be an integer")
            if ended:
                raise ValueError(f"token found after EOS at position {position}")
            if token_id == DATA_SPEC.eos_token_id:
                ended = True
            elif token_id == DATA_SPEC.bos_token_id:
                raise ValueError(
                    f"BOS is not valid in a completion at position {position}"
                )
            elif 0 <= token_id < DATA_SPEC.regular_piece_count:
                pieces.append(self._pieces[token_id])
            else:
                raise ValueError(
                    f"invalid token ID at position {position}: {token_id!r}"
                )
        return "".join(pieces)

    def utf16_piece_lengths(self) -> tuple[int, ...]:
        return tuple(java_utf16_length(piece) for piece in self._pieces)


def _atoms(text: str) -> Iterator[str]:
    yield from _ATOM_PATTERN.findall(text)


def train_tokenizer(texts: Iterable[str]) -> GreedyStringPieceTokenizer:
    atom_counts: Counter[str] = Counter()
    scalars: set[str] = set()
    text_count = 0
    for text_count, text in enumerate(texts, start=1):
        if not isinstance(text, str):
            raise TypeError(f"training text {text_count - 1} is not a string")
        _validate_scalar_text(text, role=f"training text {text_count - 1}")
        scalars.update(text)
        atom_counts.update(_atoms(text))
    if text_count == 0:
        raise ValueError("cannot train a tokenizer from an empty corpus")
    if not scalars:
        raise ValueError("cannot train a tokenizer from only empty strings")
    target = DATA_SPEC.regular_piece_count
    if len(scalars) > target:
        raise ValueError(
            f"corpus has {len(scalars)} distinct scalars but only {target} regular IDs"
        )

    pieces = sorted(scalars, key=_piece_sort_key)
    piece_to_id = {piece: token_id for token_id, piece in enumerate(pieces)}
    sequences: Counter[tuple[int, ...]] = Counter()
    for atom, count in atom_counts.items():
        sequences[tuple(piece_to_id[scalar] for scalar in atom)] += count

    while len(pieces) < target:
        pair_counts: Counter[tuple[int, int]] = Counter()
        for sequence, count in sequences.items():
            pair_counts.update(
                {
                    pair: occurrences * count
                    for pair, occurrences in Counter(pairwise(sequence)).items()
                }
            )
        candidates = [
            (pair, count, pieces[pair[0]] + pieces[pair[1]])
            for pair, count in pair_counts.items()
            if pieces[pair[0]] + pieces[pair[1]] not in piece_to_id
        ]
        if not candidates:
            raise ValueError(
                f"corpus yields only {len(pieces)} distinct pieces; expected {target}"
            )
        pair, _, merged_piece = min(
            candidates,
            key=lambda candidate: (-candidate[1], _piece_sort_key(candidate[2])),
        )
        merged_id = len(pieces)
        pieces.append(merged_piece)
        piece_to_id[merged_piece] = merged_id

        merged_sequences: Counter[tuple[int, ...]] = Counter()
        for sequence, count in sequences.items():
            output: list[int] = []
            index = 0
            while index < len(sequence):
                if index + 1 < len(sequence) and sequence[index : index + 2] == pair:
                    output.append(merged_id)
                    index += 2
                else:
                    output.append(sequence[index])
                    index += 1
            merged_sequences[tuple(output)] += count
        sequences = merged_sequences

    return GreedyStringPieceTokenizer(pieces)
