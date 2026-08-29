from __future__ import annotations

import math
from dataclasses import asdict, dataclass
from typing import Final, Literal

SCHEMA_VERSION: Final = 1
DATA_SCHEMA_VERSION: Final = 2
DATA_ABI_ID: Final = "worldless_transformer/gsp512_c256_w64_v1"
BASELINE_ARCHITECTURE_ID: Final = (
    "worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1"
)
EFFICIENT_ARCHITECTURE_ID: Final = "worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff96_untied_ve13_c256_w64_v1"
EFFICIENT_Q4_ARCHITECTURE_ID: Final = "worldless_transformer/relu2_alibi_gsp512_l4_d96_q4_kv1_h24_ff96_untied_ve13_ad24_c256_w64_v1"
Architecture = Literal["baseline", "efficient", "efficient_q4"]
ARCHITECTURE_CHOICES: Final = ("baseline", "efficient", "efficient_q4")


@dataclass(frozen=True, slots=True)
class DataSpec:
    schema_version: int = DATA_SCHEMA_VERSION
    data_abi_id: str = DATA_ABI_ID
    tokenizer_kind: str = "greedy_string_piece"
    vocab_size: int = 512
    bos_token_id: int = 510
    eos_token_id: int = 511
    context_length: int = 256
    attention_window: int = 64

    def __post_init__(self) -> None:
        if self.schema_version != DATA_SCHEMA_VERSION:
            raise ValueError(f"data schema_version must be {DATA_SCHEMA_VERSION}")
        if self.data_abi_id != DATA_ABI_ID:
            raise ValueError(f"data_abi_id must be {DATA_ABI_ID!r}")
        if self.tokenizer_kind != "greedy_string_piece":
            raise ValueError("tokenizer_kind must be 'greedy_string_piece'")
        if self.vocab_size != 512:
            raise ValueError("vocab_size must be 512")
        if (self.bos_token_id, self.eos_token_id) != (510, 511):
            raise ValueError("BOS and EOS token IDs must be 510 and 511")
        if (self.context_length, self.attention_window) != (256, 64):
            raise ValueError("context_length and attention_window must be 256 and 64")

    @property
    def regular_piece_count(self) -> int:
        return self.vocab_size - 2


DATA_SPEC: Final = DataSpec()


@dataclass(frozen=True, slots=True)
class ModelSpec:
    architecture_id: str
    d_ff: int
    schema_version: int = SCHEMA_VERSION
    layers: int = 4
    d_model: int = 96
    q_heads: int = 6
    kv_heads: int = 1
    head_dim: int = 16

    def __post_init__(self) -> None:
        if self.schema_version != SCHEMA_VERSION:
            raise ValueError(f"schema_version must be {SCHEMA_VERSION}")
        if self.architecture_id == BASELINE_ARCHITECTURE_ID:
            required_d_ff = 192
            required_attention = (6, 16)
        elif self.architecture_id == EFFICIENT_ARCHITECTURE_ID:
            required_d_ff = 96
            required_attention = (6, 16)
        elif self.architecture_id == EFFICIENT_Q4_ARCHITECTURE_ID:
            required_d_ff = 96
            required_attention = (4, 24)
        else:
            raise ValueError("architecture_id must identify a known architecture")
        if (self.layers, self.d_model, self.kv_heads) != (4, 96, 1):
            raise ValueError("model dimensions must use L4 d96 kv1")
        if (self.q_heads, self.head_dim) != required_attention:
            raise ValueError(
                f"q_heads and head_dim must be {required_attention} for "
                f"architecture {self.architecture_id!r}"
            )
        if self.d_ff != required_d_ff:
            raise ValueError(
                f"d_ff must be {required_d_ff} for architecture {self.architecture_id!r}"
            )

    @property
    def tokenizer_kind(self) -> str:
        return DATA_SPEC.tokenizer_kind

    @property
    def vocab_size(self) -> int:
        return DATA_SPEC.vocab_size

    @property
    def bos_token_id(self) -> int:
        return DATA_SPEC.bos_token_id

    @property
    def eos_token_id(self) -> int:
        return DATA_SPEC.eos_token_id

    @property
    def context_length(self) -> int:
        return DATA_SPEC.context_length

    @property
    def attention_window(self) -> int:
        return DATA_SPEC.attention_window

    @property
    def regular_piece_count(self) -> int:
        return DATA_SPEC.regular_piece_count

    @property
    def query_heads_per_kv_head(self) -> int:
        return self.q_heads // self.kv_heads

    @property
    def architecture(self) -> Architecture:
        if self.architecture_id == BASELINE_ARCHITECTURE_ID:
            return "baseline"
        if self.architecture_id == EFFICIENT_ARCHITECTURE_ID:
            return "efficient"
        return "efficient_q4"

    @property
    def alibi_slopes(self) -> tuple[tuple[int, int], ...]:
        if self.architecture == "efficient_q4":
            return ((1, 4), (1, 16), (1, 64), (1, 256))
        return ((1, 4), (1, 16), (1, 64), (1, 256), (1, 2), (1, 8))

    @property
    def tied_lm_head(self) -> bool:
        return self.architecture == "baseline"

    @property
    def value_embedding_layers(self) -> tuple[int, ...]:
        if self.architecture == "baseline":
            return ()
        return (1, 3)

    @property
    def runtime_attention_logit_denominator(self) -> int:
        if self.architecture == "efficient_q4":
            return 24
        return 16

    @property
    def data_pack_runtime_compatible(self) -> bool:
        return True

    def to_dict(self) -> dict[str, int | str]:
        return {
            **asdict(self),
            "tokenizer_kind": self.tokenizer_kind,
            "vocab_size": self.vocab_size,
            "bos_token_id": self.bos_token_id,
            "eos_token_id": self.eos_token_id,
            "context_length": self.context_length,
            "attention_window": self.attention_window,
        }


BASELINE_SPEC: Final = ModelSpec(
    architecture_id=BASELINE_ARCHITECTURE_ID,
    d_ff=192,
)
EFFICIENT_SPEC: Final = ModelSpec(
    architecture_id=EFFICIENT_ARCHITECTURE_ID,
    d_ff=96,
)
EFFICIENT_Q4_SPEC: Final = ModelSpec(
    architecture_id=EFFICIENT_Q4_ARCHITECTURE_ID,
    d_ff=96,
    q_heads=4,
    head_dim=24,
)


def spec_for_architecture(architecture: str) -> ModelSpec:
    if architecture == "baseline":
        return BASELINE_SPEC
    if architecture == "efficient":
        return EFFICIENT_SPEC
    if architecture == "efficient_q4":
        return EFFICIENT_Q4_SPEC
    raise ValueError(f"architecture must be one of {ARCHITECTURE_CHOICES}")


def spec_for_architecture_id(architecture_id: object) -> ModelSpec:
    if architecture_id == BASELINE_ARCHITECTURE_ID:
        return BASELINE_SPEC
    if architecture_id == EFFICIENT_ARCHITECTURE_ID:
        return EFFICIENT_SPEC
    if architecture_id == EFFICIENT_Q4_ARCHITECTURE_ID:
        return EFFICIENT_Q4_SPEC
    raise ValueError("architecture_id must identify a known architecture")


def _data_contract(spec: ModelSpec) -> tuple[str, int, int, int, int, int]:
    return (
        spec.tokenizer_kind,
        spec.vocab_size,
        spec.bos_token_id,
        spec.eos_token_id,
        spec.context_length,
        spec.attention_window,
    )


if _data_contract(BASELINE_SPEC) != _data_contract(EFFICIENT_SPEC) or _data_contract(
    BASELINE_SPEC
) != _data_contract(EFFICIENT_Q4_SPEC):
    raise AssertionError("known architectures must share the fixed tokenizer/data ABI")
if _data_contract(BASELINE_SPEC) != (
    DATA_SPEC.tokenizer_kind,
    DATA_SPEC.vocab_size,
    DATA_SPEC.bos_token_id,
    DATA_SPEC.eos_token_id,
    DATA_SPEC.context_length,
    DATA_SPEC.attention_window,
):
    raise AssertionError("known architectures must use DATA_SPEC")


def expected_weight_shapes(spec: ModelSpec) -> dict[str, tuple[int, int]]:
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
            }
        )
        if layer in spec.value_embedding_layers:
            shapes[f"{prefix}.attention.value_embedding.weight"] = (
                spec.vocab_size,
                spec.kv_heads * spec.head_dim,
            )
        shapes.update(
            {
                f"{prefix}.attention.out_proj.weight": (
                    spec.d_model,
                    spec.q_heads * spec.head_dim,
                ),
                f"{prefix}.ffn.up_proj.weight": (spec.d_ff, spec.d_model),
                f"{prefix}.ffn.down_proj.weight": (spec.d_model, spec.d_ff),
            }
        )
    if not spec.tied_lm_head:
        shapes["lm_head.weight"] = (spec.vocab_size, spec.d_model)
    return shapes


def zero_shift_weight_names(spec: ModelSpec) -> frozenset[str]:
    names = {"token_embedding.weight"}
    names.update(
        f"blocks.{layer}.attention.value_embedding.weight"
        for layer in spec.value_embedding_layers
    )
    if not spec.tied_lm_head:
        names.add("lm_head.weight")
    return frozenset(names)


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
ATTENTION_LOGIT_DENOMINATOR_CANDIDATES: Final = (8, 11, 16, 24, 32)
SOFTMAX_MIN_DIFFERENCE: Final = -255
TRAINING_LOGIT_SHIFT: Final = 7

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


def exp_q15_table(logit_denominator: int) -> tuple[int, ...]:
    if (
        not isinstance(logit_denominator, int)
        or isinstance(logit_denominator, bool)
        or logit_denominator not in ATTENTION_LOGIT_DENOMINATOR_CANDIDATES
    ):
        raise ValueError(
            "attention logit denominator must be one of "
            f"{ATTENTION_LOGIT_DENOMINATOR_CANDIDATES}"
        )
    return tuple(
        max(
            1,
            math.floor(
                ((1 << SOFTMAX_FRACTION_BITS) - 1)
                * math.exp(difference / logit_denominator)
                + 0.5
            ),
        )
        for difference in range(SOFTMAX_MIN_DIFFERENCE, 1)
    )
