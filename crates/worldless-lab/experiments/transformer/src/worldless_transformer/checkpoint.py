from __future__ import annotations

from pathlib import Path
from typing import Final

import torch

from .model import Transformer
from .spec import ARCHITECTURE_ID, SCHEMA_VERSION
from .tokenizer import tokenizer_id_to_int_array

_CHECKPOINT_KEYS: Final = {
    "architecture_id",
    "model",
    "schema_version",
    "step",
    "tokenizer_id",
}


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
                    "architecture_id": ARCHITECTURE_ID,
                    "model": model.state_dict(),
                    "schema_version": SCHEMA_VERSION,
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
        "architecture_id": ARCHITECTURE_ID,
        "schema_version": SCHEMA_VERSION,
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
    model = Transformer()
    model.load_state_dict(value["model"], strict=True)
    try:
        model.runtime_state()
    except (TypeError, ValueError) as error:
        raise ValueError(
            f"checkpoint model is not runtime-exportable: {error}"
        ) from error
    return model, step
