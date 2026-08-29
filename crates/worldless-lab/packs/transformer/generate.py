from __future__ import annotations

import argparse
import json
import shutil
from dataclasses import dataclass, field
from pathlib import Path

from worldless_transformer.spec import (
    ALIBI_SLOPES,
    ATTENTION_SCORE_SHIFT,
    EXP_Q15_TABLE,
    MODEL_SPEC,
    RMS_GAIN_FRACTION_BITS,
    RMS_GAIN_TABLE,
    expected_weight_shapes,
)
from worldless_transformer.tokenizer import (
    GreedyStringPieceTokenizer,
    java_utf16_length,
)

PACK_ROOT = Path(__file__).resolve().parent
FUNCTION_ROOT = Path("data/transformer/function")


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


def _product(*operands: object) -> dict[str, object]:
    return {"type": "product", "operands": list(operands)}


def _sum(operands: list[object]) -> dict[str, object]:
    return {"type": "sum", "operands": operands}


def _clamped_score(name: str) -> dict[str, object]:
    return {
        "type": "maximum",
        "operands": [
            -127,
            {"type": "minimum", "operands": [127, _score(name)]},
        ],
    }


def _round_shift_provider(source: object, shift: int) -> object:
    if shift == 0:
        return source
    half = 1 << (shift - 1)
    clamped = {
        "type": "maximum",
        "operands": [-half, {"type": "minimum", "operands": [half, source]}],
    }
    provider: object = {"type": "average", "operands": [source, clamped]}
    for _ in range(shift - 1):
        provider = {"type": "average", "operands": [provider, 0]}
    return provider


def _requantized_int8_provider(source: object, shift: int) -> object:
    return {
        "type": "maximum",
        "operands": [
            -127,
            {
                "type": "minimum",
                "operands": [127, _round_shift_provider(source, shift)],
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
        f"execute store result score {name} transformer run compute default "
        f"{_json(provider)} integer"
    )


def _append_clamped(path: str) -> str:
    return (
        f"data modify storage transformer:runtime {path} append compute default "
        f"{_json(_clamped_score('#requant'))} integer"
    )


def _generate_constants(root: Path, tokenizer: GreedyStringPieceTokenizer) -> None:
    if any(numerator != 1 for numerator, _ in ALIBI_SLOPES):
        raise RuntimeError("the data-pack ALiBi generator requires unit numerators")
    lines = [
        "data modify storage transformer:constants tokenizer_id set value "
        + _snbt_int_array(tokenizer.tokenizer_id_int_array),
        "data modify storage transformer:constants zero64 set value "
        + _snbt_int_array([0] * MODEL_SPEC.attention_window),
        "data modify storage transformer:constants zero_kv set value ["
        + ",".join(_snbt_int_array([0] * MODEL_SPEC.head_dim) for _ in range(64))
        + "]",
        "data modify storage transformer:constants rms_gain set value "
        + _snbt_int_array(RMS_GAIN_TABLE),
        "data modify storage transformer:constants softmax set value "
        + _snbt_int_array(EXP_Q15_TABLE),
    ]
    _write(root, FUNCTION_ROOT / "constants/load.mcfunction", lines)


def _dense_row(
    *,
    source_path: str,
    matrix_path: str,
    row: int,
    input_width: int,
    relu_squared: bool,
) -> object:
    terms: list[object] = []
    for column in range(input_width):
        source = _storage("transformer:runtime", f"{source_path}[{column}]")
        weight = _storage(
            "transformer:runtime", f"{matrix_path}[{row * input_width + column}]"
        )
        if relu_squared:
            positive = {"type": "maximum", "operands": [0, source]}
            terms.append(_product(positive, positive, weight))
        else:
            terms.append(_product(source, weight))
    return _sum(terms)


def _generate_projection(
    root: Path,
    name: str,
    output_width: int,
    input_width: int,
    *,
    relu_squared: bool = False,
) -> None:
    source_path = "state.up" if relu_squared else "state.source"
    lines = ["data modify storage transformer:runtime state.projected set value [I;]"]
    for row in range(output_width):
        lines.extend(
            (
                "data modify storage transformer:runtime state.acc set compute default "
                + _json(
                    _dense_row(
                        source_path=source_path,
                        matrix_path="state.matrix",
                        row=row,
                        input_width=input_width,
                        relu_squared=relu_squared,
                    )
                )
                + " integer",
                "function transformer:core/generated/requant/append_projected.macro with storage transformer:runtime state.macro",
            )
        )
    _write(root, FUNCTION_ROOT / f"core/generated/project/{name}.mcfunction", lines)


def _generate_rms(root: Path) -> None:
    squares = []
    for index in range(MODEL_SPEC.d_model):
        value = _storage("transformer:runtime", f"state.hidden[{index}]")
        squares.append(_product(value, value))
    _write(
        root,
        FUNCTION_ROOT / "core/generated/rms/sum.mcfunction",
        [_compute_to_score("#sum_square", _sum(squares))],
    )
    lines = ["data modify storage transformer:runtime state.norm set value [I;]"]
    for index in range(MODEL_SPEC.d_model):
        product = _product(
            _storage("transformer:runtime", f"state.hidden[{index}]"),
            _score("#gain"),
        )
        lines.extend(
            (
                "data modify storage transformer:runtime state.acc set compute default "
                + _json(product)
                + " integer",
                "data modify storage transformer:runtime state.norm append compute default "
                + _json(
                    _requantized_int8_provider(
                        _storage("transformer:runtime", "state.acc"),
                        RMS_GAIN_FRACTION_BITS,
                    )
                )
                + " integer",
            )
        )
    _write(root, FUNCTION_ROOT / "core/generated/rms/normalize.mcfunction", lines)


def _generate_residual(root: Path) -> None:
    lines = ["data modify storage transformer:runtime state.hidden set value [I;]"]
    for index in range(MODEL_SPEC.d_model):
        provider = {
            "type": "maximum",
            "operands": [
                -127,
                {
                    "type": "minimum",
                    "operands": [
                        127,
                        _sum(
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
            "data modify storage transformer:runtime state.hidden append compute default "
            + _json(provider)
            + " integer"
        )
    _write(root, FUNCTION_ROOT / "core/generated/residual/run.mcfunction", lines)


def _generate_attention(root: Path) -> None:
    _write(
        root,
        FUNCTION_ROOT / "core/generated/attention/qk_dispatch.macro.mcfunction",
        ["$function transformer:core/generated/attention/qk_$(head)_$(key)"],
    )
    for head in range(MODEL_SPEC.q_heads):
        q_base = head * MODEL_SPEC.head_dim
        for key in range(MODEL_SPEC.attention_window):
            terms = []
            for dimension in range(MODEL_SPEC.head_dim):
                terms.append(
                    _product(
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
                -16 * ALIBI_SLOPES[head][0] * (MODEL_SPEC.attention_window - 1 - key),
                ALIBI_SLOPES[head][1],
            )
            lines = [
                "data modify storage transformer:runtime state.acc set compute default "
                + _json(_sum(terms))
                + " integer",
                "execute store result score #score transformer run compute default "
                + _json(
                    _round_shift_provider(
                        _storage("transformer:runtime", "state.acc"),
                        ATTENTION_SCORE_SHIFT,
                    )
                )
                + " integer",
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
    for dimension in range(MODEL_SPEC.head_dim):
        terms = []
        for key in range(MODEL_SPEC.attention_window):
            terms.append(
                _product(
                    _storage("transformer:runtime", f"state.weights[{key}]"),
                    _storage(
                        "transformer:runtime",
                        f"state.layer_cache.v[{key}][{dimension}]",
                    ),
                )
            )
        lines.extend(
            (
                _compute_to_score("#acc", _sum(terms)),
                "function transformer:core/requant/divide",
                _append_clamped("state.attention"),
            )
        )
    _write(root, FUNCTION_ROOT / "core/generated/attention/value.mcfunction", lines)


def _generate_requantizers(root: Path) -> None:
    _write(
        root,
        FUNCTION_ROOT / "core/generated/requant/append_projected.macro.mcfunction",
        ["$function transformer:core/generated/requant/projected_s$(shift)"],
    )
    source = _storage("transformer:runtime", "state.acc")
    for shift in range(31):
        _write(
            root,
            FUNCTION_ROOT / f"core/generated/requant/projected_s{shift}.mcfunction",
            [
                "data modify storage transformer:runtime state.projected append compute default "
                + _json(_requantized_int8_provider(source, shift))
                + " integer"
            ],
        )


def _generate_logits(root: Path) -> None:
    lines = [
        "scoreboard players set #logit_max transformer -2147483648",
        "scoreboard players set #next_token transformer 0",
    ]
    for token in range(MODEL_SPEC.vocab_size):
        terms = []
        base = token * MODEL_SPEC.d_model
        for dimension in range(MODEL_SPEC.d_model):
            terms.append(
                _product(
                    _storage("transformer:runtime", f"state.norm[{dimension}]"),
                    _storage(
                        "transformer:model",
                        f'weights."token_embedding.weight"[{base + dimension}]',
                    ),
                )
            )
        lines.extend(
            (
                _compute_to_score("#acc", _sum(terms)),
                f"execute if score #acc transformer > #logit_max transformer run scoreboard players set #next_token transformer {token}",
                "execute if score #acc transformer > #logit_max transformer run scoreboard players operation #logit_max transformer = #acc transformer",
            )
        )
    _write(root, FUNCTION_ROOT / "core/generated/logits/run.mcfunction", lines)


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


def _generate_model_validator(root: Path) -> None:
    shapes = expected_weight_shapes()
    lines = [
        "scoreboard players set #valid transformer 1",
        "execute unless data storage transformer:model {abi:{schema:1,architecture_id:"
        + _json(MODEL_SPEC.architecture_id)
        + ",tokenizer_kind:"
        + _json(MODEL_SPEC.tokenizer_kind)
        + f",vocab_size:{MODEL_SPEC.vocab_size},bos_id:{MODEL_SPEC.bos_token_id},eos_id:{MODEL_SPEC.eos_token_id}"
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
    lines.extend(
        (
            'execute store result score #shift transformer run data get storage transformer:model shifts."token_embedding.weight"[0]',
            "execute unless score #shift transformer matches 0 run scoreboard players set #valid transformer 0",
            "execute unless score #valid transformer matches 1 run return 0",
            "return run scoreboard players get #valid transformer",
        )
    )
    _write(root, FUNCTION_ROOT / "model/validate.mcfunction", lines)

    chunk_size = 192
    for count in sorted({shape[0] * shape[1] for shape in shapes.values()}):
        range_lines: list[str] = []
        for start in range(0, count, chunk_size):
            operands = [
                _storage("transformer:validation", f"matrix[{index}]")
                for index in range(start, min(start + chunk_size, count))
            ]
            range_lines.extend(
                (
                    _compute_to_score(
                        "#minimum", {"type": "minimum", "operands": operands}
                    ),
                    "execute if score #minimum transformer matches ..-128 run scoreboard players set #valid transformer 0",
                )
            )
        _write(
            root,
            FUNCTION_ROOT / f"model/generated/validate_range_{count}.mcfunction",
            range_lines,
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


def _generate_all(root: Path, tokenizer: GreedyStringPieceTokenizer) -> None:
    core_generated = root / FUNCTION_ROOT / "core/generated"
    if core_generated.exists():
        shutil.rmtree(core_generated)
    model_generated = root / FUNCTION_ROOT / "model/generated"
    if model_generated.exists():
        shutil.rmtree(model_generated)
    _generate_constants(root, tokenizer)
    _generate_projection(root, "p96x96", 96, 96)
    _generate_projection(root, "p16x96", 16, 96)
    _generate_projection(root, "p192x96", 192, 96)
    _generate_projection(root, "p96x192_relu2", 96, 192, relu_squared=True)
    _generate_rms(root)
    _generate_residual(root)
    _generate_attention(root)
    _generate_requantizers(root)
    _generate_logits(root)
    _generate_model_validator(root)
    _generate_tokenizer(root, tokenizer)


def generate(tokenizer_path: Path, output: Path) -> None:
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
    _generate_all(output, tokenizer)
    shutil.copyfile(tokenizer_path, output / "tokenizer.json")
    (output / "tokenizer.sha256").write_text(
        tokenizer.tokenizer_id + "\n", encoding="ascii"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Compile a validated greedy StringPiece tokenizer into a transformer pack."
    )
    parser.add_argument("tokenizer_json", type=Path)
    parser.add_argument("output_dir", type=Path)
    arguments = parser.parse_args()
    generate(arguments.tokenizer_json, arguments.output_dir)


if __name__ == "__main__":
    main()
