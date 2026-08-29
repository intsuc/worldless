from __future__ import annotations

from pathlib import Path
from typing import Final

import torch

from .model import Transformer
from .spec import exp_q15_table, spec_for_architecture_id
from .tokenizer import tokenizer_id_to_int_array

_CHECKPOINT_KEYS: Final = {
    "architecture_id",
    "attention_logit_denominator",
    "model",
    "schema_version",
    "step",
    "tokenizer_id",
}
_CHECKPOINT_SCHEMA_VERSION: Final = 2


def save_checkpoint(
    path: str | Path,
    *,
    model: Transformer,
    step: int,
    tokenizer_id: str,
) -> None:
    target = Path(path)
    if target.exists():
        raise FileExistsError(f"refusing to replace checkpoint: {target}")
    if not isinstance(step, int) or isinstance(step, bool) or step <= 0:
        raise ValueError("checkpoint step must be positive")
    tokenizer_id_to_int_array(tokenizer_id)
    model.runtime_state()
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        with target.open("xb") as output:
            torch.save(
                {
                    "architecture_id": model.spec.architecture_id,
                    "attention_logit_denominator": model.attention_logit_denominator,
                    "model": model.state_dict(),
                    "schema_version": _CHECKPOINT_SCHEMA_VERSION,
                    "step": step,
                    "tokenizer_id": tokenizer_id,
                },
                output,
            )
    except BaseException:
        target.unlink(missing_ok=True)
        raise


def load_checkpoint(
    path: str | Path,
    *,
    expected_tokenizer_id: str,
) -> tuple[Transformer, int]:
    tokenizer_id_to_int_array(expected_tokenizer_id)
    value = torch.load(Path(path), map_location="cpu", weights_only=True)
    if not isinstance(value, dict) or set(value) != _CHECKPOINT_KEYS:
        raise ValueError("checkpoint has an invalid schema")
    expected = {
        "schema_version": _CHECKPOINT_SCHEMA_VERSION,
        "tokenizer_id": expected_tokenizer_id,
    }
    for field, required in expected.items():
        if value[field] != required:
            raise ValueError(
                f"checkpoint {field} must be {required!r}, got {value[field]!r}"
            )
    step = value["step"]
    if not isinstance(step, int) or isinstance(step, bool) or step <= 0:
        raise ValueError("checkpoint step must be a positive integer")
    if not isinstance(value["model"], dict):
        raise TypeError("checkpoint model must be a state dictionary")
    try:
        spec = spec_for_architecture_id(value["architecture_id"])
    except ValueError as error:
        raise ValueError(
            f"checkpoint architecture_id is not supported: {value['architecture_id']!r}"
        ) from error
    attention_logit_denominator = value["attention_logit_denominator"]
    try:
        exp_q15_table(attention_logit_denominator)
    except ValueError as error:
        raise ValueError(
            "checkpoint attention_logit_denominator is not supported: "
            f"{attention_logit_denominator!r}"
        ) from error
    model = Transformer(spec, attention_logit_denominator=attention_logit_denominator)
    model.load_state_dict(value["model"], strict=True)
    try:
        model.runtime_state()
    except (TypeError, ValueError) as error:
        raise ValueError(
            f"checkpoint model is not a valid architecture state: {error}"
        ) from error
    return model, step
