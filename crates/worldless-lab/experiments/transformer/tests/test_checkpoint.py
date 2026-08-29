from __future__ import annotations

import pytest
import torch

from worldless_transformer.checkpoint import load_checkpoint, save_checkpoint
from worldless_transformer.model import Transformer
from worldless_transformer.spec import BASELINE_SPEC, EFFICIENT_Q4_SPEC, EFFICIENT_SPEC


@pytest.mark.parametrize(
    ("spec", "attention_logit_denominator"),
    [(BASELINE_SPEC, 11), (EFFICIENT_SPEC, 24), (EFFICIENT_Q4_SPEC, 24)],
)
def test_checkpoint_round_trip_selects_the_exact_known_architecture(
    tmp_path, tokenizer, spec, attention_logit_denominator: int
) -> None:
    path = tmp_path / f"{spec.architecture}.pt"
    model = Transformer(spec, attention_logit_denominator=attention_logit_denominator)

    save_checkpoint(
        path,
        model=model,
        step=7,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    loaded, step = load_checkpoint(path, expected_tokenizer_id=tokenizer.tokenizer_id)

    assert loaded.spec == spec
    assert loaded.attention_logit_denominator == attention_logit_denominator
    assert step == 7
    assert set(loaded.state_dict()) == set(model.state_dict())
    if attention_logit_denominator == spec.runtime_attention_logit_denominator:
        loaded.require_runtime_compatible()
    else:
        with pytest.raises(ValueError, match="architecture runtime denominator"):
            loaded.require_runtime_compatible()


def test_checkpoint_rejects_an_unknown_architecture_id(tmp_path, tokenizer) -> None:
    source = tmp_path / "baseline.pt"
    save_checkpoint(
        source,
        model=Transformer(BASELINE_SPEC),
        step=1,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    value = torch.load(source, map_location="cpu", weights_only=True)
    value["architecture_id"] = "worldless_transformer/unknown"
    invalid = tmp_path / "unknown.pt"
    torch.save(value, invalid)

    with pytest.raises(ValueError, match="architecture_id is not supported"):
        load_checkpoint(invalid, expected_tokenizer_id=tokenizer.tokenizer_id)


@pytest.mark.parametrize("invalid_denominator", [True, 13])
def test_checkpoint_rejects_an_invalid_attention_denominator(
    tmp_path, tokenizer, invalid_denominator
) -> None:
    source = tmp_path / "baseline.pt"
    save_checkpoint(
        source,
        model=Transformer(BASELINE_SPEC),
        step=1,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    value = torch.load(source, map_location="cpu", weights_only=True)
    value["attention_logit_denominator"] = invalid_denominator
    invalid = tmp_path / f"denominator-{invalid_denominator}.pt"
    torch.save(value, invalid)

    with pytest.raises(ValueError, match="attention_logit_denominator"):
        load_checkpoint(invalid, expected_tokenizer_id=tokenizer.tokenizer_id)


def test_checkpoint_rejects_the_old_five_field_schema(tmp_path, tokenizer) -> None:
    source = tmp_path / "baseline.pt"
    save_checkpoint(
        source,
        model=Transformer(BASELINE_SPEC),
        step=1,
        tokenizer_id=tokenizer.tokenizer_id,
    )
    value = torch.load(source, map_location="cpu", weights_only=True)
    del value["attention_logit_denominator"]
    invalid = tmp_path / "five-field.pt"
    torch.save(value, invalid)

    with pytest.raises(ValueError, match="invalid schema"):
        load_checkpoint(invalid, expected_tokenizer_id=tokenizer.tokenizer_id)
