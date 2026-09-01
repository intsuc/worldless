from __future__ import annotations

import argparse
import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from worldless_transformer.spec import (
    ARCHITECTURE_CHOICES,
    ATTENTION_SCORE_SHIFT,
    RMS_GAIN_FRACTION_BITS,
    RMS_GAIN_TABLE,
    SOFTMAX_MIN_DIFFERENCE,
    ModelSpec,
    exp_q15_table,
    expected_weight_shapes,
    spec_for_architecture,
    zero_shift_weight_names,
)
from worldless_transformer.tokenizer import (
    GreedyStringPieceTokenizer,
    java_utf16_length,
)

PACK_ROOT = Path(__file__).resolve().parent
FUNCTION_ROOT = Path("data/transformer/function")


@dataclass(frozen=True, slots=True)
class _ProjectionSpec:
    code: str
    weight_suffix: str
    output_width: int
    input_width: int
    source_path: str
    output_path: str


def _projections(spec: ModelSpec) -> tuple[_ProjectionSpec, ...]:
    return (
        _ProjectionSpec(
            "q",
            "attention.q_proj.weight",
            spec.q_heads * spec.head_dim,
            spec.d_model,
            "state.norm",
            "state.q",
        ),
        _ProjectionSpec(
            "k",
            "attention.k_proj.weight",
            spec.kv_heads * spec.head_dim,
            spec.d_model,
            "state.norm",
            "state.k",
        ),
        _ProjectionSpec(
            "v",
            "attention.v_proj.weight",
            spec.kv_heads * spec.head_dim,
            spec.d_model,
            "state.norm",
            "state.v",
        ),
        _ProjectionSpec(
            "o",
            "attention.out_proj.weight",
            spec.d_model,
            spec.q_heads * spec.head_dim,
            "state.attention",
            "state.attention_projection",
        ),
        _ProjectionSpec(
            "u",
            "ffn.up_proj.weight",
            spec.d_ff,
            spec.d_model,
            "state.norm",
            "state.up",
        ),
        _ProjectionSpec(
            "d",
            "ffn.down_proj.weight",
            spec.d_model,
            spec.d_ff,
            "state.up_squared",
            "state.ffn_projection",
        ),
    )


def _json(value: object) -> str:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def _snbt_int_array(values: object) -> str:
    return "[I;" + ",".join(str(value) for value in values) + "]"


def _write(root: Path, relative: str | Path, lines: list[str] | str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    text = lines if isinstance(lines, str) else "\n".join(lines) + "\n"
    path.write_text(text, encoding="utf-8")


def _storage(storage: str, path: str) -> dict[str, object]:
    return {"type": "storage", "storage": storage, "path": path}


def _score(name: str) -> dict[str, object]:
    return {
        "type": "score",
        "target": {"type": "fixed", "name": name},
        "score": "transformer",
    }


def _mul(*inputs: object) -> dict[str, object]:
    return {"type": "mul", "inputs": list(inputs)}


def _add(inputs: list[object]) -> dict[str, object]:
    return {"type": "add", "inputs": inputs}


def _clamped_score(name: str) -> dict[str, object]:
    return {
        "type": "max",
        "inputs": [
            -127,
            {"type": "min", "inputs": [127, _score(name)]},
        ],
    }


def _round_shift_provider(source: object, shift: int) -> object:
    if shift == 0:
        return source
    half = 1 << (shift - 1)
    clamped = {
        "type": "max",
        "inputs": [-half, {"type": "min", "inputs": [half, source]}],
    }
    provider: object = {"type": "avg", "inputs": [source, clamped]}
    for _ in range(shift - 1):
        provider = {"type": "avg", "inputs": [provider, 0]}
    return provider


def _requantized_int8_provider(source: object, shift: int) -> object:
    return {
        "type": "max",
        "inputs": [
            -127,
            {
                "type": "min",
                "inputs": [127, _round_shift_provider(source, shift)],
            },
        ],
    }


def _round_half_away_ratio(numerator: int, denominator: int) -> int:
    magnitude, remainder = divmod(abs(numerator), denominator)
    if remainder * 2 >= denominator:
        magnitude += 1
    return magnitude if numerator >= 0 else -magnitude


def _compute_to_score(name: str, provider: object) -> str:
    return (
        f"execute store result score {name} transformer run compute default integer "
        f"{_json(provider)}"
    )


def _append_clamped(path: str) -> str:
    return (
        f"data modify storage transformer:runtime {path} append compute default integer "
        f"{_json(_clamped_score('#requant'))}"
    )


def _generate_constants(
    root: Path, tokenizer: GreedyStringPieceTokenizer, spec: ModelSpec
) -> None:
    if any(numerator != 1 for numerator, _ in spec.alibi_slopes):
        raise RuntimeError("the data-pack ALiBi generator requires unit numerators")
    lines = [
        "data modify storage transformer:constants tokenizer_id set value "
        + _snbt_int_array(tokenizer.tokenizer_id_int_array),
        "data modify storage transformer:constants zero64 set value "
        + _snbt_int_array([0] * spec.attention_window),
        "data modify storage transformer:constants zero_kv set value ["
        + ",".join(
            _snbt_int_array([0] * spec.head_dim) for _ in range(spec.attention_window)
        )
        + "]",
        "data modify storage transformer:constants rms_gain set value "
        + _snbt_int_array(RMS_GAIN_TABLE),
        "data modify storage transformer:constants softmax set value "
        + _snbt_int_array(exp_q15_table(spec.runtime_attention_logit_denominator)),
    ]
    _write(root, FUNCTION_ROOT / "constants/load.mcfunction", lines)


def _generate_setup(root: Path, spec: ModelSpec) -> None:
    lines = [
        "scoreboard objectives add transformer dummy",
        "scoreboard players set #zero transformer 0",
        "scoreboard players set #one transformer 1",
        "scoreboard players set #two transformer 2",
        "scoreboard players set #-one transformer -1",
        f"scoreboard players set #vocab transformer {spec.vocab_size}",
        f"scoreboard players set #bos transformer {spec.bos_token_id}",
        f"scoreboard players set #eos transformer {spec.eos_token_id}",
        f"scoreboard players set #layers transformer {spec.layers}",
        f"scoreboard players set #d_model transformer {spec.d_model}",
        f"scoreboard players set #q_heads transformer {spec.q_heads}",
        f"scoreboard players set #head_dim transformer {spec.head_dim}",
        f"scoreboard players set #d_ff transformer {spec.d_ff}",
        f"scoreboard players set #context transformer {spec.context_length}",
        f"scoreboard players set #window transformer {spec.attention_window}",
        f"scoreboard players set #cache_last transformer {spec.attention_window - 1}",
        f"scoreboard players set #softmax_min transformer {SOFTMAX_MIN_DIFFERENCE}",
        "data modify storage transformer:runtime active_bank set value -1",
        "function transformer:constants/load",
    ]
    _write(root, FUNCTION_ROOT / "setup.mcfunction", lines)


def _generate_layer(root: Path, spec: ModelSpec) -> None:
    lines = [
        "function transformer:core/rms/run",
        "function transformer:core/project/q",
        "function transformer:core/project/k",
        "function transformer:core/project/v",
        "execute store result storage transformer:runtime state.macro.layer int 1 run scoreboard players get #layer transformer",
    ]
    lines.extend(
        f"execute if score #layer transformer matches {layer} run function transformer:core/value_embedding/run"
        for layer in spec.value_embedding_layers
    )
    lines.extend(
        (
            "function transformer:core/load_layer_cache.macro with storage transformer:runtime state.macro",
            "data remove storage transformer:runtime state.layer_cache.k[0]",
            "data remove storage transformer:runtime state.layer_cache.v[0]",
            "data modify storage transformer:runtime state.layer_cache.k append from storage transformer:runtime state.k",
            "data modify storage transformer:runtime state.layer_cache.v append from storage transformer:runtime state.v",
            "function transformer:core/attention/run",
            "function transformer:core/save_layer_cache.macro with storage transformer:runtime state.macro",
            "function transformer:core/project/o",
            "data modify storage transformer:runtime state.delta set from storage transformer:runtime state.attention_projection",
            "function transformer:core/residual",
            "",
            "function transformer:core/rms/run",
            "function transformer:core/project/up",
            "function transformer:core/project/down",
            "data modify storage transformer:runtime state.delta set from storage transformer:runtime state.ffn_projection",
            "function transformer:core/residual",
            "",
            "scoreboard players add #layer transformer 1",
            "execute if score #layer transformer < #layers transformer run function transformer:core/layer",
        )
    )
    _write(root, FUNCTION_ROOT / "core/layer.mcfunction", lines)


def _dense_row(
    *,
    source_path: str,
    matrix_storage: str,
    matrix_path: str,
    row: int,
    input_width: int,
) -> object:
    terms: list[object] = []
    for column in range(input_width):
        source = _storage("transformer:runtime", f"{source_path}[{column}]")
        weight = _storage(
            matrix_storage, f"{matrix_path}[{row * input_width + column}]"
        )
        terms.append(_mul(source, weight))
    return _add(terms)


def _generate_projection(
    root: Path,
    projection: _ProjectionSpec,
) -> None:
    lines = [
        f"data modify storage transformer:runtime {projection.output_path} set value [I;]"
    ]
    for row in range(projection.output_width):
        lines.extend(
            (
                "$data modify storage transformer:runtime state.acc set compute default integer "
                + _json(
                    _dense_row(
                        source_path=projection.source_path,
                        matrix_storage="$(s)",
                        matrix_path="$(w)",
                        row=row,
                        input_width=projection.input_width,
                    )
                ),
                f"$data modify storage transformer:runtime {projection.output_path} append compute default integer $(rq)",
            )
        )
    _write(
        root,
        FUNCTION_ROOT / f"core/generated/project/{projection.code}.mcfunction",
        lines,
    )


def _generate_relu_squared(root: Path, spec: ModelSpec) -> None:
    lines = ["data modify storage transformer:runtime state.up_squared set value [I;]"]
    for index in range(spec.d_ff):
        source = _storage("transformer:runtime", f"state.up[{index}]")
        positive = {"type": "max", "inputs": [0, source]}
        lines.append(
            "data modify storage transformer:runtime state.up_squared append compute default integer "
            + _json(_mul(positive, positive))
        )
    _write(root, FUNCTION_ROOT / "core/generated/relu_squared/run.mcfunction", lines)


def _generate_rms(root: Path, spec: ModelSpec) -> None:
    squares = []
    for index in range(spec.d_model):
        value = _storage("transformer:runtime", f"state.hidden[{index}]")
        squares.append(_mul(value, value))
    _write(
        root,
        FUNCTION_ROOT / "core/generated/rms/sum.mcfunction",
        [_compute_to_score("#sum_square", _add(squares))],
    )
    lines = ["data modify storage transformer:runtime state.norm set value [I;]"]
    for index in range(spec.d_model):
        product = _mul(
            _storage("transformer:runtime", f"state.hidden[{index}]"),
            _score("#gain"),
        )
        lines.extend(
            (
                "data modify storage transformer:runtime state.acc set compute default integer "
                + _json(product),
                "data modify storage transformer:runtime state.norm append compute default integer "
                + _json(
                    _requantized_int8_provider(
                        _storage("transformer:runtime", "state.acc"),
                        RMS_GAIN_FRACTION_BITS,
                    )
                ),
            )
        )
    _write(root, FUNCTION_ROOT / "core/generated/rms/normalize.mcfunction", lines)


def _generate_residual(root: Path, spec: ModelSpec) -> None:
    lines = ["data modify storage transformer:runtime state.hidden set value [I;]"]
    for index in range(spec.d_model):
        provider = {
            "type": "max",
            "inputs": [
                -127,
                {
                    "type": "min",
                    "inputs": [
                        127,
                        _add(
                            [
                                _storage(
                                    "transformer:runtime", f"state.residual[{index}]"
                                ),
                                _storage(
                                    "transformer:runtime", f"state.delta[{index}]"
                                ),
                            ]
                        ),
                    ],
                },
            ],
        }
        lines.append(
            "data modify storage transformer:runtime state.hidden append compute default integer "
            + _json(provider)
        )
    _write(root, FUNCTION_ROOT / "core/generated/residual/run.mcfunction", lines)


def _generate_attention(root: Path, spec: ModelSpec) -> None:
    _write(
        root,
        FUNCTION_ROOT / "core/generated/attention/qk_dispatch.macro.mcfunction",
        ["$function transformer:core/generated/attention/qk_$(head)_$(key)"],
    )
    for head in range(spec.q_heads):
        q_base = head * spec.head_dim
        for key in range(spec.attention_window):
            terms = []
            for dimension in range(spec.head_dim):
                terms.append(
                    _mul(
                        _storage(
                            "transformer:runtime", f"state.q[{q_base + dimension}]"
                        ),
                        _storage(
                            "transformer:runtime",
                            f"state.layer_cache.k[{key}][{dimension}]",
                        ),
                    )
                )
            bias = _round_half_away_ratio(
                -spec.runtime_attention_logit_denominator
                * spec.alibi_slopes[head][0]
                * (spec.attention_window - 1 - key),
                spec.alibi_slopes[head][1],
            )
            lines = [
                "data modify storage transformer:runtime state.acc set compute default integer "
                + _json(_add(terms)),
                "execute store result score #score transformer run compute default integer "
                + _json(
                    _round_shift_provider(
                        _storage("transformer:runtime", "state.acc"),
                        ATTENTION_SCORE_SHIFT,
                    )
                ),
            ]
            if bias < 0:
                lines.append(f"scoreboard players remove #score transformer {-bias}")
            elif bias > 0:
                lines.append(f"scoreboard players add #score transformer {bias}")
            _write(
                root,
                FUNCTION_ROOT / f"core/generated/attention/qk_{head}_{key}.mcfunction",
                lines,
            )

    lines: list[str] = []
    for dimension in range(spec.head_dim):
        terms = []
        for key in range(spec.attention_window):
            terms.append(
                _mul(
                    _storage("transformer:runtime", f"state.weights[{key}]"),
                    _storage(
                        "transformer:runtime",
                        f"state.layer_cache.v[{key}][{dimension}]",
                    ),
                )
            )
        lines.extend(
            (
                _compute_to_score("#acc", _add(terms)),
                "function transformer:core/requant/divide",
                _append_clamped("state.attention"),
            )
        )
    _write(root, FUNCTION_ROOT / "core/generated/attention/value.mcfunction", lines)


def _generate_value_embedding(root: Path, spec: ModelSpec) -> None:
    runtime_directory = root / FUNCTION_ROOT / "core/value_embedding"
    if runtime_directory.exists():
        shutil.rmtree(runtime_directory)
    if not spec.value_embedding_layers:
        return

    _write(
        root,
        FUNCTION_ROOT / "core/value_embedding/run.mcfunction",
        [
            "execute store result storage transformer:runtime state.macro.token int 1 run scoreboard players get #token transformer",
            "function transformer:core/generated/value_embedding/dispatch.macro with storage transformer:runtime state.macro",
        ],
    )
    _write(
        root,
        FUNCTION_ROOT / "core/generated/value_embedding/dispatch.macro.mcfunction",
        [
            "$function transformer:core/generated/value_embedding/token_$(token).macro with storage transformer:runtime state.macro"
        ],
    )
    width = spec.kv_heads * spec.head_dim
    for token in range(spec.vocab_size):
        base = token * width
        lines = []
        for dimension in range(width):
            source = _add(
                [
                    _storage("transformer:runtime", f"state.v[{dimension}]"),
                    _storage(
                        "transformer:a$(bank)",
                        f"ve$(layer)[{base + dimension}]",
                    ),
                ]
            )
            lines.append(
                f"$data modify storage transformer:runtime state.v[{dimension}] set compute default integer "
                + _json(_requantized_int8_provider(source, 0))
            )
        _write(
            root,
            FUNCTION_ROOT
            / f"core/generated/value_embedding/token_{token}.macro.mcfunction",
            lines,
        )


def _generate_logits(root: Path, spec: ModelSpec) -> None:
    _write(
        root,
        FUNCTION_ROOT / "core/generated/logits/dispatch.macro.mcfunction",
        ["$function transformer:core/generated/logits/run_a$(bank)"],
    )
    for bank in range(2):
        lines = [
            "scoreboard players set #logit_max transformer -2147483648",
            "scoreboard players set #next_token transformer 0",
        ]
        weight_path = "e" if spec.tied_lm_head else "l"
        for token in range(spec.vocab_size):
            base = token * spec.d_model
            terms = [
                _mul(
                    _storage("transformer:runtime", f"state.norm[{dimension}]"),
                    _storage(
                        f"transformer:a{bank}",
                        f"{weight_path}[{base + dimension}]",
                    ),
                )
                for dimension in range(spec.d_model)
            ]
            lines.extend(
                (
                    _compute_to_score("#acc", _add(terms)),
                    f"execute if score #acc transformer > #logit_max transformer run scoreboard players set #next_token transformer {token}",
                    "execute if score #acc transformer > #logit_max transformer run scoreboard players operation #logit_max transformer = #acc transformer",
                )
            )
        _write(
            root,
            FUNCTION_ROOT / f"core/generated/logits/run_a{bank}.mcfunction",
            lines,
        )


def _empty_array_type_probe(path: str, tag: str) -> list[str]:
    return [
        f"data modify storage transformer:validation probe set from storage transformer:model {path}",
        "data remove storage transformer:validation probe[]",
        f"execute unless data storage transformer:validation {{probe:{tag}}} run scoreboard players set #valid transformer 0",
    ]


def _size_check(path: str, size: int) -> list[str]:
    return [
        "scoreboard players set #actual transformer -1",
        f"execute store result score #actual transformer run data get storage transformer:model {path}",
        f"execute unless score #actual transformer matches {size} run scoreboard players set #valid transformer 0",
    ]


def _generate_model_validator(root: Path, spec: ModelSpec) -> None:
    shapes = expected_weight_shapes(spec)
    lines = [
        "scoreboard players set #valid transformer 1",
        "execute unless data storage transformer:model {abi:{schema:"
        + str(spec.schema_version)
        + ",architecture_id:"
        + _json(spec.architecture_id)
        + ",tokenizer_kind:"
        + _json(spec.tokenizer_kind)
        + f",vocab_size:{spec.vocab_size},bos_id:{spec.bos_token_id},eos_id:{spec.eos_token_id}"
        + "}} run scoreboard players set #valid transformer 0",
        "data modify storage transformer:validation root set from storage transformer:model",
        "data remove storage transformer:validation root.abi",
        "data remove storage transformer:validation root.weights",
        "data remove storage transformer:validation root.biases",
        "data remove storage transformer:validation root.shifts",
        "scoreboard players set #actual transformer -1",
        "execute store result score #actual transformer run data get storage transformer:validation root",
        "execute unless score #actual transformer matches 0 run scoreboard players set #valid transformer 0",
        *_size_check("abi", 7),
        *_size_check("weights", len(shapes)),
        *_size_check("shifts", len(shapes)),
        *_size_check("biases", 0),
        "data modify storage transformer:validation probe set from storage transformer:model biases",
        "execute unless data storage transformer:validation {probe:{}} run scoreboard players set #valid transformer 0",
        *_size_check("abi.tokenizer_id", 8),
        *_empty_array_type_probe("abi.tokenizer_id", "[I;]"),
    ]
    for name, shape in shapes.items():
        size = shape[0] * shape[1]
        quoted = _json(name)
        lines.extend(_size_check(f"weights.{quoted}", size))
        lines.extend(_empty_array_type_probe(f"weights.{quoted}", "[B;]"))
        lines.extend(_size_check(f"shifts.{quoted}", 1))
        lines.extend(_empty_array_type_probe(f"shifts.{quoted}", "[I;]"))
        lines.extend(
            (
                f"execute store result score #shift transformer run data get storage transformer:model shifts.{quoted}[0]",
                "execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0",
                f"data modify storage transformer:validation matrix set from storage transformer:model weights.{quoted}",
                f"function transformer:model/generated/validate_range_{size}",
            )
        )
    for name in sorted(zero_shift_weight_names(spec)):
        quoted = _json(name)
        lines.extend(
            (
                f"execute store result score #shift transformer run data get storage transformer:model shifts.{quoted}[0]",
                "execute unless score #shift transformer matches 0 run scoreboard players set #valid transformer 0",
            )
        )
    lines.extend(
        (
            "execute unless score #valid transformer matches 1 run return 0",
            "return run scoreboard players get #valid transformer",
        )
    )
    _write(root, FUNCTION_ROOT / "model/validate.mcfunction", lines)

    chunk_size = 192
    for count in sorted({shape[0] * shape[1] for shape in shapes.values()}):
        range_lines: list[str] = []
        for start in range(0, count, chunk_size):
            inputs = [
                _storage("transformer:validation", f"matrix[{index}]")
                for index in range(start, min(start + chunk_size, count))
            ]
            range_lines.extend(
                (
                    _compute_to_score(
                        "#minimum", {"type": "min", "inputs": inputs}
                    ),
                    "execute if score #minimum transformer matches ..-128 run scoreboard players set #valid transformer 0",
                )
            )
        _write(
            root,
            FUNCTION_ROOT / f"model/generated/validate_range_{count}.mcfunction",
            range_lines,
        )


def _projection_weight_name(layer: int, projection: _ProjectionSpec) -> str:
    return f"blocks.{layer}.{projection.weight_suffix}"


def _active_weight_name(layer: int, projection: _ProjectionSpec) -> str:
    return f"w{layer}{projection.code}"


def _active_arguments_name(layer: int, projection: _ProjectionSpec) -> str:
    return f"a{layer}{projection.code}"


def _value_embedding_weight_name(layer: int) -> str:
    return f"blocks.{layer}.attention.value_embedding.weight"


def _value_embedding_active_name(layer: int) -> str:
    return f"ve{layer}"


def _generate_model_activation(root: Path, spec: ModelSpec) -> None:
    projections = _projections(spec)
    expected_names = set(expected_weight_shapes(spec))
    staged_names = {"token_embedding.weight"}
    staged_names.update(
        _projection_weight_name(layer, projection)
        for layer in range(spec.layers)
        for projection in projections
    )
    staged_names.update(
        _value_embedding_weight_name(layer) for layer in spec.value_embedding_layers
    )
    if not spec.tied_lm_head:
        staged_names.add("lm_head.weight")
    if staged_names != expected_names:
        raise RuntimeError("active model staging does not cover the model ABI")

    source = _storage("transformer:runtime", "state.acc")
    _write(
        root,
        FUNCTION_ROOT / "model/generated/stage_shift_dispatch.macro.mcfunction",
        ["$function transformer:model/generated/stage_shift_$(shift)"],
    )
    for shift in range(31):
        provider = _json(_requantized_int8_provider(source, shift))
        _write(
            root,
            FUNCTION_ROOT / f"model/generated/stage_shift_{shift}.mcfunction",
            [
                "data modify storage transformer:validation rq set value "
                + _json(provider)
            ],
        )

    _write(
        root,
        FUNCTION_ROOT / "model/generated/stage_dispatch.macro.mcfunction",
        ["$function transformer:model/generated/stage_a$(bank)"],
    )
    for bank in range(2):
        storage = f"transformer:a{bank}"
        lines = [
            f'data modify storage {storage} e set from storage transformer:model weights."token_embedding.weight"',
            f"data modify storage {storage} t set from storage transformer:model abi.tokenizer_id",
            f"data modify storage {storage} b set from storage transformer:model abi.bos_id",
        ]
        if not spec.tied_lm_head:
            lines.append(
                f'data modify storage {storage} l set from storage transformer:model weights."lm_head.weight"'
            )
        for layer in spec.value_embedding_layers:
            weight_name = _value_embedding_weight_name(layer)
            lines.append(
                f"data modify storage {storage} {_value_embedding_active_name(layer)} set from storage transformer:model weights.{_json(weight_name)}"
            )
        for layer in range(spec.layers):
            for projection in projections:
                weight_name = _projection_weight_name(layer, projection)
                weight_path = _active_weight_name(layer, projection)
                arguments_path = _active_arguments_name(layer, projection)
                quoted_weight_name = _json(weight_name)
                lines.extend(
                    (
                        f"data modify storage {storage} {weight_path} set from storage transformer:model weights.{quoted_weight_name}",
                        f"data modify storage {storage} {arguments_path} set value "
                        + _json({"s": storage, "w": weight_path}),
                        (
                            "data modify storage transformer:validation macro.shift set from storage transformer:model "
                            f"shifts.{quoted_weight_name}[0]"
                        ),
                        "function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro",
                        f"data modify storage {storage} {arguments_path}.rq set from storage transformer:validation rq",
                    )
                )
        _write(
            root,
            FUNCTION_ROOT / f"model/generated/stage_a{bank}.mcfunction",
            lines,
        )

    _write(
        root,
        FUNCTION_ROOT / "model/activate.mcfunction",
        [
            "function transformer:model/validate",
            "execute unless score #valid transformer matches 1 run return fail",
            "data modify storage transformer:validation macro.bank set value 0",
            "execute if data storage transformer:runtime {active_bank:0} run data modify storage transformer:validation macro.bank set value 1",
            "function transformer:model/generated/stage_dispatch.macro with storage transformer:validation macro",
            "return run data modify storage transformer:runtime active_bank set from storage transformer:validation macro.bank",
        ],
    )


@dataclass
class _TrieNode:
    children: dict[str, _TrieNode] = field(default_factory=dict)
    token_id: int | None = None
    state_id: int = -1


def _utf16_sort_key(value: str) -> bytes:
    return value.encode("utf-16-be")


def _generate_tokenizer(root: Path, tokenizer: GreedyStringPieceTokenizer) -> None:
    trie = _TrieNode()
    for token_id, piece in enumerate(tokenizer.pieces):
        node = trie
        for scalar in piece:
            node = node.children.setdefault(scalar, _TrieNode())
        node.token_id = token_id

    states: list[tuple[_TrieNode, int]] = []

    def assign(node: _TrieNode, incoming_units: int) -> None:
        node.state_id = len(states)
        states.append((node, incoming_units))
        for scalar, child in sorted(
            node.children.items(), key=lambda item: _utf16_sort_key(item[0])
        ):
            assign(child, java_utf16_length(scalar))

    assign(trie, 0)
    state_directory = root / FUNCTION_ROOT / "tokenize/state"
    if state_directory.exists():
        shutil.rmtree(state_directory)
    state_directory.mkdir(parents=True)

    for node, incoming_units in states:
        lines: list[str] = []
        if node is not trie:
            lines.append(
                "data modify storage transformer:runtime state.scan set string storage transformer:runtime state.scan "
                + str(incoming_units)
            )
        if node.token_id is not None:
            lines.extend(
                (
                    f"data modify storage transformer:runtime state.best_id set value {node.token_id}",
                    "data modify storage transformer:runtime state.best_remaining set from storage transformer:runtime state.scan",
                )
            )
        lines.append(
            'execute if data storage transformer:runtime {state:{scan:""}} run return 1'
        )
        for scalar, child in sorted(
            node.children.items(), key=lambda item: _utf16_sort_key(item[0])
        ):
            units = java_utf16_length(scalar)
            literal = _json(scalar)
            lines.extend(
                (
                    f"data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 {units}",
                    f"execute if data storage transformer:runtime {{state:{{ch:{literal}}}}} run return run function transformer:tokenize/state/{child.state_id}",
                )
            )
        lines.append("return 0" if node is trie else "return 1")
        _write(
            root,
            FUNCTION_ROOT / f"tokenize/state/{node.state_id}.mcfunction",
            lines,
        )

    for token_id, piece in enumerate(tokenizer.pieces):
        if tokenizer.encode(piece) != [token_id]:
            raise RuntimeError(
                f"generated tokenizer self-check failed for piece {token_id}"
            )
    loop_path = root / FUNCTION_ROOT / "tokenize/loop.mcfunction"
    loop_text = loop_path.read_text(encoding="utf-8")
    loop_text = loop_text.replace(
        "function transformer:tokenize/state/root",
        "function transformer:tokenize/state/0",
    )
    if "function transformer:tokenize/state/0" not in loop_text:
        raise RuntimeError("tokenizer loop does not contain the generated root call")
    loop_path.write_text(loop_text, encoding="utf-8")


def _generate_all(
    root: Path, tokenizer: GreedyStringPieceTokenizer, spec: ModelSpec
) -> None:
    core_generated = root / FUNCTION_ROOT / "core/generated"
    if core_generated.exists():
        shutil.rmtree(core_generated)
    model_generated = root / FUNCTION_ROOT / "model/generated"
    if model_generated.exists():
        shutil.rmtree(model_generated)
    _generate_setup(root, spec)
    _generate_layer(root, spec)
    _generate_constants(root, tokenizer, spec)
    for projection in _projections(spec):
        _generate_projection(root, projection)
    _generate_relu_squared(root, spec)
    _generate_rms(root, spec)
    _generate_residual(root, spec)
    _generate_attention(root, spec)
    _generate_value_embedding(root, spec)
    _generate_logits(root, spec)
    _generate_model_validator(root, spec)
    _generate_model_activation(root, spec)
    _generate_tokenizer(root, tokenizer)


def generate(tokenizer_path: Path, output: Path, architecture: str) -> None:
    if architecture not in ARCHITECTURE_CHOICES:
        raise ValueError(f"architecture must be one of {ARCHITECTURE_CHOICES}")
    spec = spec_for_architecture(architecture)
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    resolved_output = output.resolve()
    if output.exists():
        raise FileExistsError(f"refusing to replace output directory: {output}")
    if resolved_output.is_relative_to(PACK_ROOT):
        raise ValueError("output directory must not be inside the template pack")
    shutil.copytree(
        PACK_ROOT,
        output,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    shutil.rmtree(output / FUNCTION_ROOT / "fixture")
    shutil.rmtree(output / "data/worldless_lab")
    _generate_all(output, tokenizer, spec)
    shutil.copyfile(tokenizer_path, output / "tokenizer.json")
    (output / "tokenizer.sha256").write_text(
        tokenizer.tokenizer_id + "\n", encoding="ascii"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compile a validated tokenizer and architecture into a transformer pack."
    )
    parser.add_argument("--architecture", choices=ARCHITECTURE_CHOICES, required=True)
    parser.add_argument("tokenizer_json", type=Path)
    parser.add_argument("output_dir", type=Path)
    arguments = parser.parse_args()
    generate(arguments.tokenizer_json, arguments.output_dir, arguments.architecture)


if __name__ == "__main__":
    main()
