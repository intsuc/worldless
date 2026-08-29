from __future__ import annotations

import argparse
import json
from pathlib import Path

from .artifact import ModelArtifact
from .checkpoint import load_checkpoint
from .data import iter_tinystories, write_token_stream
from .export_nbt import write_command_storage
from .model import Transformer
from .reference import ExactRuntimeReference
from .spec import MODEL_SPEC
from .tokenizer import GreedyStringPieceTokenizer, train_tokenizer
from .training import TrainConfig, evaluate_checkpoint, train


def _positive_integer(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="worldless-transformer")
    commands = parser.add_subparsers(dest="command", required=True)

    tokenizer = commands.add_parser("train-tokenizer")
    tokenizer.add_argument("--output", type=Path, required=True)
    tokenizer.add_argument("--limit", type=_positive_integer)

    preprocess = commands.add_parser("preprocess")
    preprocess.add_argument("--tokenizer", type=Path, required=True)
    preprocess.add_argument("--split", choices=("train", "validation"), required=True)
    preprocess.add_argument("--output", type=Path, required=True)
    preprocess.add_argument("--limit", type=_positive_integer)

    train_parser = commands.add_parser("train")
    train_parser.add_argument("--tokenizer", type=Path, required=True)
    train_parser.add_argument("--train-tokens", type=Path, required=True)
    train_parser.add_argument("--validation-tokens", type=Path, required=True)
    train_parser.add_argument("--output", type=Path, required=True)
    train_parser.add_argument("--batch-size", type=_positive_integer, required=True)
    train_parser.add_argument("--learning-rate", type=float, required=True)
    train_parser.add_argument("--seed", type=int, required=True)
    train_parser.add_argument("--device", required=True)
    train_parser.add_argument(
        "--mode", choices=("float", "fake_runtime"), default="fake_runtime"
    )
    train_parser.add_argument(
        "--validation-batches", type=_positive_integer, required=True
    )

    evaluate = commands.add_parser("evaluate")
    evaluate.add_argument("--tokenizer", type=Path, required=True)
    evaluate.add_argument("--validation-tokens", type=Path, required=True)
    evaluate.add_argument("--checkpoint", type=Path, required=True)
    evaluate.add_argument("--batch-size", type=_positive_integer, required=True)
    evaluate.add_argument("--batches", type=_positive_integer, required=True)
    evaluate.add_argument("--seed", type=int, required=True)
    evaluate.add_argument("--device", required=True)
    evaluate.add_argument(
        "--mode", choices=("float", "fake_runtime"), default="fake_runtime"
    )

    trace = commands.add_parser("trace")
    trace.add_argument("--tokenizer", type=Path, required=True)
    trace.add_argument("--checkpoint", type=Path, required=True)
    trace.add_argument("--prefix", required=True)
    trace.add_argument("--output", type=Path, required=True)

    generate = commands.add_parser("generate")
    generate.add_argument("--tokenizer", type=Path, required=True)
    generate.add_argument("--checkpoint", type=Path, required=True)
    generate.add_argument("--prefix", required=True)
    generate.add_argument("--max-new-tokens", type=_positive_integer, required=True)

    export = commands.add_parser("export")
    export.add_argument("--tokenizer", type=Path, required=True)
    export.add_argument("--checkpoint", type=Path, required=True)
    export.add_argument("--output", type=Path, required=True)
    export.add_argument("--storage-path", default="model")
    export.add_argument("--uncompressed", action="store_true")

    commands.add_parser("spec")
    return parser


def _load_model_and_tokenizer(
    tokenizer_path: Path, checkpoint_path: Path
) -> tuple[GreedyStringPieceTokenizer, Transformer]:
    tokenizer = GreedyStringPieceTokenizer.load(tokenizer_path)
    model, _ = load_checkpoint(
        checkpoint_path, expected_tokenizer_id=tokenizer.tokenizer_id
    )
    model.eval()
    return tokenizer, model


def _prefix_tokens(tokenizer: GreedyStringPieceTokenizer, prefix: str) -> list[int]:
    tokens = [MODEL_SPEC.bos_token_id, *tokenizer.encode(prefix)]
    if len(tokens) > MODEL_SPEC.context_length:
        raise ValueError(
            f"prefix encodes to {len(tokens)} tokens; maximum is {MODEL_SPEC.context_length}"
        )
    return tokens


def main() -> None:
    arguments = _parser().parse_args()
    if arguments.command == "train-tokenizer":
        tokenizer = train_tokenizer(iter_tinystories("train", limit=arguments.limit))
        tokenizer.save(arguments.output)
        print(json.dumps({"tokenizer_id": tokenizer.tokenizer_id}, sort_keys=True))
    elif arguments.command == "preprocess":
        tokenizer = GreedyStringPieceTokenizer.load(arguments.tokenizer)
        metadata = write_token_stream(
            arguments.output,
            split=arguments.split,
            tokenizer=tokenizer,
            limit=arguments.limit,
        )
        print(json.dumps(metadata, sort_keys=True))
    elif arguments.command == "train":
        train(
            tokenizer_path=arguments.tokenizer,
            train_tokens=arguments.train_tokens,
            validation_tokens=arguments.validation_tokens,
            output_checkpoint=arguments.output,
            config=TrainConfig(
                batch_size=arguments.batch_size,
                learning_rate=arguments.learning_rate,
                seed=arguments.seed,
                device=arguments.device,
                mode=arguments.mode,
                validation_batches=arguments.validation_batches,
            ),
        )
    elif arguments.command == "evaluate":
        result = evaluate_checkpoint(
            tokenizer_path=arguments.tokenizer,
            validation_tokens=arguments.validation_tokens,
            checkpoint_path=arguments.checkpoint,
            batch_size=arguments.batch_size,
            batches=arguments.batches,
            seed=arguments.seed,
            device_name=arguments.device,
            mode=arguments.mode,
        )
        print(json.dumps(result, sort_keys=True))
    elif arguments.command == "trace":
        tokenizer, model = _load_model_and_tokenizer(
            arguments.tokenizer, arguments.checkpoint
        )
        tokens = _prefix_tokens(tokenizer, arguments.prefix)
        trace = ExactRuntimeReference(model.runtime_state()).golden_trace(tokens)
        if arguments.output.exists():
            raise FileExistsError(f"refusing to replace trace: {arguments.output}")
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        with arguments.output.open("x", encoding="utf-8") as output:
            output.write(json.dumps(trace.to_dict(), sort_keys=True, indent=2) + "\n")
    elif arguments.command == "generate":
        tokenizer, model = _load_model_and_tokenizer(
            arguments.tokenizer, arguments.checkpoint
        )
        prefix_tokens = _prefix_tokens(tokenizer, arguments.prefix)
        generated = ExactRuntimeReference(model.runtime_state()).generate(
            prefix_tokens, max_new_tokens=arguments.max_new_tokens
        )
        completion = tokenizer.decode_completion(generated[len(prefix_tokens) :])
        print(
            json.dumps(
                {"completion": completion, "token_ids": generated},
                ensure_ascii=False,
                sort_keys=True,
            )
        )
    elif arguments.command == "export":
        tokenizer, model = _load_model_and_tokenizer(
            arguments.tokenizer, arguments.checkpoint
        )
        runtime_state = model.runtime_state()
        artifact = ModelArtifact.create(
            tokenizer_id=tokenizer.tokenizer_id,
            weights=runtime_state.weights,
            shifts=runtime_state.shifts,
        )
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        write_command_storage(
            arguments.output,
            artifact,
            storage_path=arguments.storage_path,
            compressed=not arguments.uncompressed,
        )
    elif arguments.command == "spec":
        model = Transformer()
        print(
            json.dumps(
                {**MODEL_SPEC.to_dict(), "parameter_count": model.parameter_count()},
                sort_keys=True,
            )
        )
    else:
        raise AssertionError(f"unhandled command: {arguments.command}")


if __name__ == "__main__":
    main()
