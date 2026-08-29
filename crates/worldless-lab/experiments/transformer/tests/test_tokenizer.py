from __future__ import annotations

import json
import random

import pytest

from worldless_transformer.tokenizer import (
    GreedyStringPieceTokenizer,
    UnsupportedTextError,
    java_utf16_length,
    tokenizer_id_to_int_array,
    train_tokenizer,
)


def test_greedy_encoding_is_lossless_and_counts_java_utf16_units(
    tokenizer: GreedyStringPieceTokenizer,
) -> None:
    token_ids = tokenizer.encode("😀aababa")

    assert token_ids[:2] == [3, 5]
    assert tokenizer.decode_text(token_ids) == "😀aababa"
    assert java_utf16_length("😀a") == 3
    assert tokenizer.utf16_piece_lengths()[3] == 2


def test_unknown_scalars_and_special_token_misuse_are_rejected(
    tokenizer: GreedyStringPieceTokenizer,
) -> None:
    with pytest.raises(UnsupportedTextError, match=r"U\+007A"):
        tokenizer.encode("z")
    with pytest.raises(ValueError, match="not a regular piece"):
        tokenizer.decode_text([510])
    with pytest.raises(ValueError, match="after EOS"):
        tokenizer.decode_completion([0, 511, 0])
    with pytest.raises(TypeError, match="must be an integer"):
        tokenizer.decode_completion([511.0])
    with pytest.raises(ValueError, match="lone UTF-16 surrogate"):
        GreedyStringPieceTokenizer(["a", "\ud800", *("a" * n for n in range(2, 510))])


def test_tokenizer_artifact_is_content_addressed_and_strict(
    tmp_path, tokenizer: GreedyStringPieceTokenizer
) -> None:
    path = tmp_path / "tokenizer.json"
    tokenizer.save(path)

    loaded = GreedyStringPieceTokenizer.load(path)
    assert loaded.pieces == tokenizer.pieces
    assert loaded.tokenizer_id == tokenizer.tokenizer_id
    assert tokenizer_id_to_int_array(tokenizer.tokenizer_id) == (
        tokenizer.tokenizer_id_int_array
    )

    value = json.loads(path.read_text(encoding="utf-8"))
    value["unknown"] = 1
    path.write_text(json.dumps(value), encoding="utf-8")
    with pytest.raises(ValueError, match="unknown=.*unknown"):
        GreedyStringPieceTokenizer.load(path)


def test_vocabulary_training_is_deterministic_and_runtime_uses_max_match() -> None:
    generator = random.Random(7)
    alphabet = "abcdefghijklmno"
    words = [
        "".join(generator.choice(alphabet) for _ in range(14)) for _ in range(1_200)
    ]
    corpus = [" ".join(words)]

    first = train_tokenizer(corpus)
    second = train_tokenizer(corpus)

    assert first.pieces == second.pieces
    assert first.decode_text(first.encode(corpus[0])) == corpus[0]
