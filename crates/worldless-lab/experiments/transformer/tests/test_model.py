from __future__ import annotations

import pytest
import torch

from worldless_transformer.model import Transformer
from worldless_transformer.quantization import round_shift_int
from worldless_transformer.reference import ExactRuntimeReference
from worldless_transformer.spec import (
    MODEL_SPEC,
    RMS_GAIN_TABLE,
    expected_weight_shapes,
)


def test_fixed_model_layout_and_parameter_count() -> None:
    model = Transformer()

    assert model.parameter_count() == 282_624
    assert set(model.runtime_state().weights) == set(expected_weight_shapes())
    assert model.runtime_state().shifts["token_embedding.weight"] == 0
    assert RMS_GAIN_TABLE[64 * 64] == 1 << 15


def test_runtime_state_rejects_non_finite_master_weights() -> None:
    model = Transformer()
    with torch.no_grad():
        model.token_embedding.weight[0, 0] = torch.nan

    with pytest.raises(ValueError, match="contains NaN or infinity"):
        model.runtime_state()


def test_power_of_two_requantization_rounds_half_away_from_zero() -> None:
    values = torch.tensor([-7, -6, -5, -4, -3, 0, 3, 4, 5, 6, 7])

    assert round_shift_int(values, 2).tolist() == [
        -2,
        -2,
        -1,
        -1,
        -1,
        0,
        1,
        1,
        1,
        2,
        2,
    ]


def test_fake_runtime_matches_independent_exact_reference() -> None:
    torch.manual_seed(11)
    model = Transformer().eval()
    token_ids = [
        MODEL_SPEC.bos_token_id,
        *(index * 17 % MODEL_SPEC.regular_piece_count for index in range(64)),
    ]

    with torch.no_grad():
        fake_logits = model(
            torch.tensor([token_ids]), mode="fake_runtime", raw_logits=True
        )[0]
    reference = ExactRuntimeReference(model.runtime_state())
    exact_logits = reference.logits(token_ids)

    assert torch.equal(fake_logits.to(torch.int32), exact_logits)
    trace = reference.golden_trace(token_ids)
    assert len(trace.layers) == MODEL_SPEC.layers
    assert all(len(layer.after_ffn) == MODEL_SPEC.d_model for layer in trace.layers)
    assert len(trace.logits) == MODEL_SPEC.vocab_size
    assert trace.next_token_id == int(exact_logits[-1].argmax())


@pytest.mark.skipif(not torch.cuda.is_available(), reason="CUDA is unavailable")
def test_fake_runtime_cuda_matches_cpu_integer_reference() -> None:
    torch.manual_seed(17)
    model = Transformer().eval()
    token_ids = [
        MODEL_SPEC.bos_token_id,
        *(index * 29 % MODEL_SPEC.regular_piece_count for index in range(64)),
    ]
    reference = ExactRuntimeReference(model.runtime_state())

    with torch.no_grad():
        cuda_logits = model.cuda()(
            torch.tensor([token_ids], device="cuda"),
            mode="fake_runtime",
            raw_logits=True,
        )[0].cpu()

    assert torch.equal(cuda_logits.to(torch.int32), reference.logits(token_ids))


def test_fake_runtime_keeps_gradients_for_every_weight() -> None:
    torch.manual_seed(13)
    model = Transformer()
    inputs = torch.randint(0, MODEL_SPEC.vocab_size, (1, 8))
    targets = torch.randint(0, MODEL_SPEC.vocab_size, (1, 8))

    logits = model(inputs, mode="fake_runtime")
    torch.nn.functional.cross_entropy(
        logits.reshape(-1, MODEL_SPEC.vocab_size), targets.reshape(-1)
    ).backward()

    assert all(parameter.grad is not None for parameter in model.parameters())
    assert all(torch.isfinite(parameter.grad).all() for parameter in model.parameters())


def test_generation_allows_one_prediction_from_a_full_context() -> None:
    model = Transformer().eval()
    prefix = torch.zeros((1, MODEL_SPEC.context_length), dtype=torch.int64)
    reference = ExactRuntimeReference(model.runtime_state())

    with torch.no_grad():
        generated = model.generate(prefix, max_new_tokens=1)
    assert generated.shape == (1, MODEL_SPEC.context_length + 1)
    assert len(reference.generate(prefix[0].tolist(), max_new_tokens=1)) == 257

    with pytest.raises(ValueError, match=r"prefix length \+ max_new_tokens - 1"):
        model.generate(prefix, max_new_tokens=2)
    with pytest.raises(ValueError, match=r"prefix length \+ max_new_tokens - 1"):
        reference.generate(prefix[0].tolist(), max_new_tokens=2)
