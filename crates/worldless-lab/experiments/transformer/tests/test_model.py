from __future__ import annotations

import pytest
import torch

from worldless_transformer.model import Transformer
from worldless_transformer.quantization import round_shift_int
from worldless_transformer.reference import ExactRuntimeReference
from worldless_transformer.spec import (
    ARCHITECTURE_CHOICES,
    ATTENTION_LOGIT_DENOMINATOR_CANDIDATES,
    BASELINE_SPEC,
    EFFICIENT_Q4_DEEP_SPEC,
    EFFICIENT_Q4_FF192_SPEC,
    EFFICIENT_Q4_SPEC,
    EFFICIENT_Q4_WIDE_SPEC,
    EFFICIENT_SPEC,
    INT8_MAX,
    INT32_MAX,
    KNOWN_MODEL_SPECS,
    RMS_GAIN_TABLE,
    SOFTMAX_FRACTION_BITS,
    expected_weight_shapes,
    spec_for_architecture,
)


def test_known_architecture_names_and_ids_are_exact() -> None:
    expected_ids = {
        "baseline": "worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1",
        "efficient": "worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff96_untied_ve13_c256_w64_v1",
        "efficient_q4": "worldless_transformer/relu2_alibi_gsp512_l4_d96_q4_kv1_h24_ff96_untied_ve13_ad24_c256_w64_v1",
        "efficient_q4_ff192": "worldless_transformer/relu2_alibi_gsp512_l4_d96_q4_kv1_h24_ff192_untied_ve13_ad24_c256_w64_v1",
        "efficient_q4_wide": "worldless_transformer/relu2_alibi_gsp512_l4_d128_q4_kv1_h32_ff128_untied_ve13_ad32_c256_w64_v1",
        "efficient_q4_deep": "worldless_transformer/relu2_alibi_gsp512_l8_d96_q4_kv1_h24_ff96_untied_ve1357_ad24_c256_w64_v1",
    }

    assert ARCHITECTURE_CHOICES == tuple(expected_ids)
    assert {
        architecture: spec_for_architecture(architecture).architecture_id
        for architecture in ARCHITECTURE_CHOICES
    } == expected_ids


@pytest.mark.parametrize(
    ("spec", "parameter_count"),
    [
        (BASELINE_SPEC, 282_624),
        (EFFICIENT_SPEC, 274_432),
        (EFFICIENT_Q4_SPEC, 288_768),
        (EFFICIENT_Q4_FF192_SPEC, 362_496),
        (EFFICIENT_Q4_WIDE_SPEC, 458_752),
        (EFFICIENT_Q4_DEEP_SPEC, 479_232),
    ],
)
def test_known_model_layouts_and_parameter_counts(spec, parameter_count: int) -> None:
    model = Transformer(spec)

    assert model.parameter_count() == parameter_count
    assert set(model.runtime_state().weights) == set(expected_weight_shapes(spec))
    assert model.runtime_state().shifts["token_embedding.weight"] == 0
    for layer_index in spec.value_embedding_layers:
        key = f"blocks.{layer_index}.attention.value_embedding.weight"
        assert expected_weight_shapes(spec)[key] == (spec.vocab_size, spec.head_dim)
        assert model.runtime_state().shifts[key] == 0
    if not spec.tied_lm_head:
        assert model.runtime_state().shifts["lm_head.weight"] == 0
    assert len(spec.alibi_slopes) == spec.q_heads
    assert RMS_GAIN_TABLE[64 * 64] == 1 << 15


def test_known_architecture_accumulators_remain_exact_and_inside_int32() -> None:
    for spec in KNOWN_MODEL_SPECS:
        int8_dot_widths = (
            spec.d_model,
            spec.q_heads * spec.head_dim,
            spec.head_dim,
        )
        for width in int8_dot_widths:
            dense_or_qk_bound = INT8_MAX**2 * width
            assert dense_or_qk_bound <= INT32_MAX
            assert dense_or_qk_bound < 2**53

        relu_squared_down_bound = INT8_MAX**3 * spec.d_ff
        assert relu_squared_down_bound <= INT32_MAX
        assert relu_squared_down_bound < 2**53

        attention_numerator_bound = (
            ((1 << SOFTMAX_FRACTION_BITS) - 1) * INT8_MAX * spec.attention_window
        )
        assert attention_numerator_bound <= INT32_MAX
        assert attention_numerator_bound < 2**53


def test_runtime_state_rejects_non_finite_master_weights() -> None:
    model = Transformer(BASELINE_SPEC)
    with torch.no_grad():
        model.token_embedding.weight[0, 0] = torch.nan

    with pytest.raises(ValueError, match="contains NaN or infinity"):
        model.runtime_state()


def test_alibi_slopes_follow_each_known_query_head_layout() -> None:
    assert (
        BASELINE_SPEC.alibi_slopes
        == EFFICIENT_SPEC.alibi_slopes
        == (
            (1, 4),
            (1, 16),
            (1, 64),
            (1, 256),
            (1, 2),
            (1, 8),
        )
    )
    for spec in (
        EFFICIENT_Q4_SPEC,
        EFFICIENT_Q4_FF192_SPEC,
        EFFICIENT_Q4_WIDE_SPEC,
        EFFICIENT_Q4_DEEP_SPEC,
    ):
        assert spec.alibi_slopes == (
            (1, 4),
            (1, 16),
            (1, 64),
            (1, 256),
        )


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


@pytest.mark.parametrize(
    "spec",
    [
        BASELINE_SPEC,
        EFFICIENT_SPEC,
        EFFICIENT_Q4_SPEC,
        EFFICIENT_Q4_FF192_SPEC,
        EFFICIENT_Q4_WIDE_SPEC,
        EFFICIENT_Q4_DEEP_SPEC,
    ],
)
def test_fake_runtime_matches_independent_exact_reference(spec) -> None:
    torch.manual_seed(11)
    model = Transformer(spec).eval()
    token_ids = [
        spec.bos_token_id,
        *(index * 17 % spec.regular_piece_count for index in range(64)),
    ]

    with torch.no_grad():
        fake_logits = model(
            torch.tensor([token_ids]), mode="fake_runtime", raw_logits=True
        )[0]
    reference = ExactRuntimeReference(model.runtime_state(), spec)
    exact_logits = reference.logits(token_ids)

    assert torch.equal(fake_logits.to(torch.int32), exact_logits)
    trace = reference.golden_trace(token_ids)
    assert trace.architecture_id == spec.architecture_id
    assert len(trace.layers) == spec.layers
    assert all(len(layer.after_ffn) == spec.d_model for layer in trace.layers)
    assert len(trace.logits) == spec.vocab_size
    assert trace.next_token_id == int(exact_logits[-1].argmax())


@pytest.mark.parametrize(
    "attention_logit_denominator", ATTENTION_LOGIT_DENOMINATOR_CANDIDATES
)
def test_attention_scale_candidates_match_the_exact_integer_reference(
    attention_logit_denominator: int,
) -> None:
    torch.manual_seed(19)
    model = Transformer(
        BASELINE_SPEC,
        attention_logit_denominator=attention_logit_denominator,
    ).eval()
    token_ids = [BASELINE_SPEC.bos_token_id, 3, 17, 91, 4]

    with torch.no_grad():
        fake_logits = model(
            torch.tensor([token_ids]), mode="fake_runtime", raw_logits=True
        )[0]
    reference = ExactRuntimeReference(
        model.runtime_state(),
        BASELINE_SPEC,
        attention_logit_denominator=attention_logit_denominator,
    )

    assert torch.equal(fake_logits.to(torch.int32), reference.logits(token_ids))


def test_only_the_runtime_attention_denominator_is_export_compatible() -> None:
    for spec in (
        BASELINE_SPEC,
        EFFICIENT_SPEC,
        EFFICIENT_Q4_SPEC,
        EFFICIENT_Q4_FF192_SPEC,
        EFFICIENT_Q4_WIDE_SPEC,
        EFFICIENT_Q4_DEEP_SPEC,
    ):
        Transformer(spec).require_runtime_compatible()

    with pytest.raises(ValueError, match="architecture runtime denominator 16"):
        Transformer(
            BASELINE_SPEC, attention_logit_denominator=11
        ).require_runtime_compatible()
    with pytest.raises(ValueError, match="architecture runtime denominator 24"):
        Transformer(
            EFFICIENT_Q4_SPEC, attention_logit_denominator=16
        ).require_runtime_compatible()
    with pytest.raises(ValueError, match="architecture runtime denominator 32"):
        Transformer(
            EFFICIENT_Q4_WIDE_SPEC, attention_logit_denominator=24
        ).require_runtime_compatible()


def test_efficient_value_embedding_adds_in_int32_and_clamps_before_attention(
    monkeypatch,
) -> None:
    model = Transformer(EFFICIENT_SPEC)
    attention = model.blocks[1].attention
    assert attention.value_embedding is not None
    with torch.no_grad():
        attention.value_embedding.weight.zero_()
        attention.value_embedding.weight[3].fill_(100)
        attention.value_embedding.weight[4].fill_(-100)
    projected = torch.stack((torch.full((16,), 100.0), torch.full((16,), -100.0))).view(
        1, 2, 16
    )
    monkeypatch.setattr(
        attention.v_proj,
        "forward",
        lambda inputs, *, mode: projected,
    )

    value = attention._project_value(
        torch.zeros((1, 2, EFFICIENT_SPEC.d_model)),
        torch.tensor([[3, 4]]),
        mode="fake_runtime",
    )

    assert value[0, 0].tolist() == [127.0] * 16
    assert value[0, 1].tolist() == [-127.0] * 16


@pytest.mark.skipif(not torch.cuda.is_available(), reason="CUDA is unavailable")
def test_fake_runtime_cuda_matches_cpu_integer_reference() -> None:
    torch.manual_seed(17)
    model = Transformer(BASELINE_SPEC).eval()
    token_ids = [
        BASELINE_SPEC.bos_token_id,
        *(index * 29 % BASELINE_SPEC.regular_piece_count for index in range(64)),
    ]
    reference = ExactRuntimeReference(model.runtime_state(), BASELINE_SPEC)

    with torch.no_grad():
        cuda_logits = model.cuda()(
            torch.tensor([token_ids], device="cuda"),
            mode="fake_runtime",
            raw_logits=True,
        )[0].cpu()

    assert torch.equal(cuda_logits.to(torch.int32), reference.logits(token_ids))


@pytest.mark.parametrize(
    "spec",
    [
        BASELINE_SPEC,
        EFFICIENT_SPEC,
        EFFICIENT_Q4_SPEC,
        EFFICIENT_Q4_FF192_SPEC,
        EFFICIENT_Q4_WIDE_SPEC,
        EFFICIENT_Q4_DEEP_SPEC,
    ],
)
def test_fake_runtime_keeps_gradients_for_every_weight(spec) -> None:
    torch.manual_seed(13)
    model = Transformer(spec)
    inputs = torch.randint(0, spec.vocab_size, (1, 8))
    targets = torch.randint(0, spec.vocab_size, (1, 8))

    logits = model(inputs, mode="fake_runtime")
    torch.nn.functional.cross_entropy(
        logits.reshape(-1, spec.vocab_size), targets.reshape(-1)
    ).backward()

    assert all(parameter.grad is not None for parameter in model.parameters())
    assert all(torch.isfinite(parameter.grad).all() for parameter in model.parameters())


def test_generation_allows_one_prediction_from_a_full_context() -> None:
    model = Transformer(BASELINE_SPEC).eval()
    prefix = torch.zeros((1, BASELINE_SPEC.context_length), dtype=torch.int64)
    reference = ExactRuntimeReference(model.runtime_state(), BASELINE_SPEC)

    with torch.no_grad():
        generated = model.generate(prefix, max_new_tokens=1)
    assert generated.shape == (1, BASELINE_SPEC.context_length + 1)
    assert len(reference.generate(prefix[0].tolist(), max_new_tokens=1)) == 257

    with pytest.raises(ValueError, match=r"prefix length \+ max_new_tokens - 1"):
        model.generate(prefix, max_new_tokens=2)
    with pytest.raises(ValueError, match=r"prefix length \+ max_new_tokens - 1"):
        reference.generate(prefix[0].tolist(), max_new_tokens=2)
