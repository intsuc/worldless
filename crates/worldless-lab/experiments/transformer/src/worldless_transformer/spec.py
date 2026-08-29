from __future__ import annotations

import math
from dataclasses import asdict, dataclass
from typing import Final

SCHEMA_VERSION: Final = 1
ARCHITECTURE_ID: Final = (
    "worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1"
)


@dataclass(frozen=True, slots=True)
class ModelSpec:
    schema_version: int = SCHEMA_VERSION
    architecture_id: str = ARCHITECTURE_ID
    tokenizer_kind: str = "greedy_string_piece"
    vocab_size: int = 512
    bos_token_id: int = 510
    eos_token_id: int = 511
    layers: int = 4
    d_model: int = 96
    q_heads: int = 6
    kv_heads: int = 1
    head_dim: int = 16
    d_ff: int = 192
    context_length: int = 256
    attention_window: int = 64

    def __post_init__(self) -> None:
        if self.schema_version != SCHEMA_VERSION:
            raise ValueError(f"schema_version must be {SCHEMA_VERSION}")
        if self.architecture_id != ARCHITECTURE_ID:
            raise ValueError(f"architecture_id must be {ARCHITECTURE_ID!r}")
        if self.tokenizer_kind != "greedy_string_piece":
            raise ValueError("tokenizer_kind must be 'greedy_string_piece'")
        if self.vocab_size != 512:
            raise ValueError("vocab_size must be 512")
        if (self.bos_token_id, self.eos_token_id) != (510, 511):
            raise ValueError("BOS and EOS token IDs must be 510 and 511")
        if self.layers <= 0 or self.d_model <= 0 or self.d_ff <= 0:
            raise ValueError("model dimensions must be positive")
        if self.d_model != self.q_heads * self.head_dim:
            raise ValueError("d_model must equal q_heads * head_dim")
        if self.q_heads % self.kv_heads != 0:
            raise ValueError("q_heads must be divisible by kv_heads")
        if not 0 < self.attention_window <= self.context_length:
            raise ValueError("attention_window must be in 1..context_length")

    @property
    def regular_piece_count(self) -> int:
        return self.vocab_size - 2

    @property
    def query_heads_per_kv_head(self) -> int:
        return self.q_heads // self.kv_heads

    def to_dict(self) -> dict[str, int | str]:
        return asdict(self)


MODEL_SPEC: Final = ModelSpec()


def expected_weight_shapes(spec: ModelSpec = MODEL_SPEC) -> dict[str, tuple[int, int]]:
    shapes = {"token_embedding.weight": (spec.vocab_size, spec.d_model)}
    for layer in range(spec.layers):
        prefix = f"blocks.{layer}"
        shapes.update(
            {
                f"{prefix}.attention.q_proj.weight": (
                    spec.q_heads * spec.head_dim,
                    spec.d_model,
                ),
                f"{prefix}.attention.k_proj.weight": (
                    spec.kv_heads * spec.head_dim,
                    spec.d_model,
                ),
                f"{prefix}.attention.v_proj.weight": (
                    spec.kv_heads * spec.head_dim,
                    spec.d_model,
                ),
                f"{prefix}.attention.out_proj.weight": (
                    spec.d_model,
                    spec.q_heads * spec.head_dim,
                ),
                f"{prefix}.ffn.up_proj.weight": (spec.d_ff, spec.d_model),
                f"{prefix}.ffn.down_proj.weight": (spec.d_model, spec.d_ff),
            }
        )
    return shapes


INT8_MIN: Final = -127
INT8_MAX: Final = 127
INT32_MIN: Final = -(1 << 31)
INT32_MAX: Final = (1 << 31) - 1
REQUANT_SHIFT_MIN: Final = 0
REQUANT_SHIFT_MAX: Final = 30
RMS_TARGET: Final = 64
RMS_GAIN_FRACTION_BITS: Final = 15
ATTENTION_SCORE_SHIFT: Final = 9
SOFTMAX_FRACTION_BITS: Final = 15
SOFTMAX_LOGIT_DENOMINATOR: Final = 16
SOFTMAX_MIN_DIFFERENCE: Final = -255
TRAINING_LOGIT_SHIFT: Final = 7

# Canonical ALiBi order for six heads, represented as exact powers of two.
ALIBI_SLOPES: Final = ((1, 4), (1, 16), (1, 64), (1, 256), (1, 2), (1, 8))

# The shift is stored once per exported tensor. These are initialization values,
# not a fallback for a missing artifact entry.
DEFAULT_DENSE_SHIFTS: Final = {
    "q_proj": 6,
    "k_proj": 6,
    "v_proj": 6,
    "out_proj": 6,
    "up_proj": 6,
    "down_proj": 11,
}


def _rms_gain(mean_square: int) -> int:
    divisor = max(mean_square, 1)
    numerator = RMS_TARGET << RMS_GAIN_FRACTION_BITS
    floor_value = math.isqrt((numerator * numerator) // divisor)
    if 4 * numerator * numerator >= divisor * (2 * floor_value + 1) ** 2:
        return floor_value + 1
    return floor_value


RMS_GAIN_TABLE: Final = tuple(_rms_gain(value) for value in range(INT8_MAX**2 + 1))
EXP_Q15_TABLE: Final = tuple(
    max(
        1,
        math.floor(
            ((1 << SOFTMAX_FRACTION_BITS) - 1)
            * math.exp(difference / SOFTMAX_LOGIT_DENOMINATOR)
            + 0.5
        ),
    )
    for difference in range(SOFTMAX_MIN_DIFFERENCE, 1)
)
