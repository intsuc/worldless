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
EFFICIENT_Q4_FF192_ARCHITECTURE_ID: Final = "worldless_transformer/relu2_alibi_gsp512_l4_d96_q4_kv1_h24_ff192_untied_ve13_ad24_c256_w64_v1"
EFFICIENT_Q4_WIDE_ARCHITECTURE_ID: Final = "worldless_transformer/relu2_alibi_gsp512_l4_d128_q4_kv1_h32_ff128_untied_ve13_ad32_c256_w64_v1"
EFFICIENT_Q4_DEEP_ARCHITECTURE_ID: Final = "worldless_transformer/relu2_alibi_gsp512_l8_d96_q4_kv1_h24_ff96_untied_ve1357_ad24_c256_w64_v1"
Architecture = Literal[
    "baseline",
    "efficient",
    "efficient_q4",
    "efficient_q4_ff192",
    "efficient_q4_wide",
    "efficient_q4_deep",
]


@dataclass(frozen=True, slots=True)
class _ArchitectureDefinition:
    architecture: Architecture
    architecture_id: str
    layers: int
    d_model: int
    q_heads: int
    kv_heads: int
    head_dim: int
    d_ff: int
    alibi_slopes: tuple[tuple[int, int], ...]
    tied_lm_head: bool
    value_embedding_layers: tuple[int, ...]
    runtime_attention_logit_denominator: int


_Q6_ALIBI_SLOPES: Final = (
    (1, 4),
    (1, 16),
    (1, 64),
    (1, 256),
    (1, 2),
    (1, 8),
)
_Q4_ALIBI_SLOPES: Final = _Q6_ALIBI_SLOPES[:4]
_ARCHITECTURE_DEFINITIONS: Final = (
    _ArchitectureDefinition(
        architecture="baseline",
        architecture_id=BASELINE_ARCHITECTURE_ID,
        layers=4,
        d_model=96,
        q_heads=6,
        kv_heads=1,
        head_dim=16,
        d_ff=192,
        alibi_slopes=_Q6_ALIBI_SLOPES,
        tied_lm_head=True,
        value_embedding_layers=(),
        runtime_attention_logit_denominator=16,
    ),
    _ArchitectureDefinition(
        architecture="efficient",
        architecture_id=EFFICIENT_ARCHITECTURE_ID,
        layers=4,
        d_model=96,
        q_heads=6,
        kv_heads=1,
        head_dim=16,
        d_ff=96,
        alibi_slopes=_Q6_ALIBI_SLOPES,
        tied_lm_head=False,
        value_embedding_layers=(1, 3),
        runtime_attention_logit_denominator=16,
    ),
    _ArchitectureDefinition(
        architecture="efficient_q4",
        architecture_id=EFFICIENT_Q4_ARCHITECTURE_ID,
        layers=4,
        d_model=96,
        q_heads=4,
        kv_heads=1,
        head_dim=24,
        d_ff=96,
        alibi_slopes=_Q4_ALIBI_SLOPES,
        tied_lm_head=False,
        value_embedding_layers=(1, 3),
        runtime_attention_logit_denominator=24,
    ),
    _ArchitectureDefinition(
        architecture="efficient_q4_ff192",
        architecture_id=EFFICIENT_Q4_FF192_ARCHITECTURE_ID,
        layers=4,
        d_model=96,
        q_heads=4,
        kv_heads=1,
        head_dim=24,
        d_ff=192,
        alibi_slopes=_Q4_ALIBI_SLOPES,
        tied_lm_head=False,
        value_embedding_layers=(1, 3),
        runtime_attention_logit_denominator=24,
    ),
    _ArchitectureDefinition(
        architecture="efficient_q4_wide",
        architecture_id=EFFICIENT_Q4_WIDE_ARCHITECTURE_ID,
        layers=4,
        d_model=128,
        q_heads=4,
        kv_heads=1,
        head_dim=32,
        d_ff=128,
        alibi_slopes=_Q4_ALIBI_SLOPES,
        tied_lm_head=False,
        value_embedding_layers=(1, 3),
        runtime_attention_logit_denominator=32,
    ),
    _ArchitectureDefinition(
        architecture="efficient_q4_deep",
        architecture_id=EFFICIENT_Q4_DEEP_ARCHITECTURE_ID,
        layers=8,
        d_model=96,
        q_heads=4,
        kv_heads=1,
        head_dim=24,
        d_ff=96,
        alibi_slopes=_Q4_ALIBI_SLOPES,
        tied_lm_head=False,
        value_embedding_layers=(1, 3, 5, 7),
        runtime_attention_logit_denominator=24,
    ),
)
ARCHITECTURE_CHOICES: Final = tuple(
    definition.architecture for definition in _ARCHITECTURE_DEFINITIONS
)


def _definition_for_architecture_id(
    architecture_id: object,
) -> _ArchitectureDefinition:
    for definition in _ARCHITECTURE_DEFINITIONS:
        if architecture_id == definition.architecture_id:
            return definition
    raise ValueError("architecture_id must identify a known architecture")


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
        definition = _definition_for_architecture_id(self.architecture_id)
        actual_layout = (
            self.layers,
            self.d_model,
            self.q_heads,
            self.kv_heads,
            self.head_dim,
            self.d_ff,
        )
        required_layout = (
            definition.layers,
            definition.d_model,
            definition.q_heads,
            definition.kv_heads,
            definition.head_dim,
            definition.d_ff,
        )
        if actual_layout != required_layout:
            raise ValueError(
                f"model dimensions must be {required_layout} for architecture "
                f"{self.architecture_id!r}"
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
        return _definition_for_architecture_id(self.architecture_id).architecture

    @property
    def alibi_slopes(self) -> tuple[tuple[int, int], ...]:
        return _definition_for_architecture_id(self.architecture_id).alibi_slopes

    @property
    def tied_lm_head(self) -> bool:
        return _definition_for_architecture_id(self.architecture_id).tied_lm_head

    @property
    def value_embedding_layers(self) -> tuple[int, ...]:
        return _definition_for_architecture_id(
            self.architecture_id
        ).value_embedding_layers

    @property
    def runtime_attention_logit_denominator(self) -> int:
        return _definition_for_architecture_id(
            self.architecture_id
        ).runtime_attention_logit_denominator

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


def _model_spec(definition: _ArchitectureDefinition) -> ModelSpec:
    return ModelSpec(
        architecture_id=definition.architecture_id,
        d_ff=definition.d_ff,
        layers=definition.layers,
        d_model=definition.d_model,
        q_heads=definition.q_heads,
        kv_heads=definition.kv_heads,
        head_dim=definition.head_dim,
    )


KNOWN_MODEL_SPECS: Final = tuple(
    _model_spec(definition) for definition in _ARCHITECTURE_DEFINITIONS
)
(
    BASELINE_SPEC,
    EFFICIENT_SPEC,
    EFFICIENT_Q4_SPEC,
    EFFICIENT_Q4_FF192_SPEC,
    EFFICIENT_Q4_WIDE_SPEC,
    EFFICIENT_Q4_DEEP_SPEC,
) = KNOWN_MODEL_SPECS
KNOWN_MODEL_WIDTHS: Final = tuple(
    dict.fromkeys(spec.d_model for spec in KNOWN_MODEL_SPECS)
)


def spec_for_architecture(architecture: str) -> ModelSpec:
    for spec in KNOWN_MODEL_SPECS:
        if architecture == spec.architecture:
            return spec
    raise ValueError(f"architecture must be one of {ARCHITECTURE_CHOICES}")


def spec_for_architecture_id(architecture_id: object) -> ModelSpec:
    for spec in KNOWN_MODEL_SPECS:
        if architecture_id == spec.architecture_id:
            return spec
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


if any(
    _data_contract(spec) != _data_contract(BASELINE_SPEC)
    for spec in KNOWN_MODEL_SPECS[1:]
):
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
