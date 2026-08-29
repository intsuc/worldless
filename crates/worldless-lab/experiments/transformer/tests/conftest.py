from __future__ import annotations

import pytest

from worldless_transformer.tokenizer import GreedyStringPieceTokenizer


@pytest.fixture
def tokenizer() -> GreedyStringPieceTokenizer:
    pieces = ["a", "b", "c", "😀", "ab", "aab"]
    pieces.extend("c" + "a" * length + "c" for length in range(504))
    return GreedyStringPieceTokenizer(pieces)
