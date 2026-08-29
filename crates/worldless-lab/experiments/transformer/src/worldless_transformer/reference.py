from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass

import torch
from torch import Tensor

from .quantization import (
    RuntimeState,
    clamp_int8,
    requantize_int8,
    round_shift_int,
    rounded_divide_int,
    saturate_int32,
)
from .spec import (
    ALIBI_SLOPES,
    ARCHITECTURE_ID,
    ATTENTION_SCORE_SHIFT,
    EXP_Q15_TABLE,
    INT32_MIN,
    MODEL_SPEC,
    RMS_GAIN_FRACTION_BITS,
    RMS_GAIN_TABLE,
    SOFTMAX_MIN_DIFFERENCE,
    ModelSpec,
    expected_weight_shapes,
)


@dataclass(frozen=True, slots=True)
class LayerTrace:
    after_attention: tuple[int, ...]
    after_ffn: tuple[int, ...]


@dataclass(frozen=True, slots=True)
class GoldenTrace:
    architecture_id: str
    input_tokens: tuple[int, ...]
    layers: tuple[LayerTrace, ...]
    final_hidden: tuple[int, ...]
    logits: tuple[int, ...]
    next_token_id: int

    def to_dict(self) -> dict[str, object]:
        return {
            "architecture_id": self.architecture_id,
            "input_tokens": list(self.input_tokens),
            "layers": [
                {
                    "after_attention": list(layer.after_attention),
                    "after_ffn": list(layer.after_ffn),
                }
                for layer in self.layers
            ],
            "final_hidden": list(self.final_hidden),
            "logits": list(self.logits),
            "next_token_id": self.next_token_id,
        }


class ExactRuntimeReference:
    def __init__(self, state: RuntimeState, spec: ModelSpec = MODEL_SPEC) -> None:
        if spec != MODEL_SPEC:
            raise ValueError("spec must match the fixed MODEL_SPEC")
        expected = expected_weight_shapes(spec)
        if set(state.weights) != set(expected):
            raise ValueError("runtime state keys do not match architecture schema")
        self.spec = spec
        self.weights: dict[str, Tensor] = {}
        for key, shape in expected.items():
            weight = state.weights[key]
            if tuple(weight.shape) != shape:
                raise ValueError(
                    f"runtime weight {key!r} has shape {tuple(weight.shape)}, expected {shape}"
                )
            self.weights[key] = weight.detach().cpu().contiguous()
        self.shifts = dict(state.shifts)
        if self.shifts["token_embedding.weight"] != 0:
            raise ValueError("token_embedding.weight shift must be zero")
        self._rms_gain = torch.tensor(RMS_GAIN_TABLE, dtype=torch.int64)
        self._exp = torch.tensor(EXP_Q15_TABLE, dtype=torch.int64)

    def _validate_tokens(self, token_ids: Sequence[int]) -> Tensor:
        if not 0 < len(token_ids) <= self.spec.context_length:
            raise ValueError(f"token count must be in 1..{self.spec.context_length}")
        for index, token_id in enumerate(token_ids):
            if not isinstance(token_id, int) or isinstance(token_id, bool):
                raise TypeError(f"token ID at position {index} must be an integer")
            if not 0 <= token_id < self.spec.vocab_size:
                raise ValueError(
                    f"token ID at position {index} must be in "
                    f"0..{self.spec.vocab_size - 1}"
                )
        return torch.tensor(token_ids, dtype=torch.int64)

    def _norm(self, inputs: Tensor) -> Tensor:
        mean_square = torch.div(
            inputs.to(torch.int64).square().sum(dim=-1, keepdim=True),
            self.spec.d_model,
            rounding_mode="floor",
        )
        gain = self._rms_gain[mean_square]
        normalized = rounded_divide_int(
            inputs.to(torch.int64) * gain,
            torch.full_like(gain, 1 << RMS_GAIN_FRACTION_BITS),
        )
        return clamp_int8(normalized).to(torch.int8)

    def _linear(self, inputs: Tensor, key: str) -> Tensor:
        accumulator = torch.matmul(
            inputs.to(torch.int64), self.weights[key].to(torch.int64).transpose(-1, -2)
        )
        return requantize_int8(saturate_int32(accumulator), self.shifts[key])

    def _alibi_bias(self, length: int) -> Tensor:
        positions = torch.arange(length, dtype=torch.int64)
        distance = positions[:, None] - positions[None, :]
        heads: list[Tensor] = []
        for numerator, denominator in ALIBI_SLOPES:
            heads.append(
                rounded_divide_int(
                    -16 * numerator * distance,
                    torch.full_like(distance, denominator),
                )
            )
        return torch.stack(heads)

    def _attention(self, normalized: Tensor, prefix: str) -> Tensor:
        length = normalized.shape[0]
        query = self._linear(normalized, f"{prefix}.q_proj.weight").view(
            length, self.spec.q_heads, self.spec.head_dim
        )
        key = self._linear(normalized, f"{prefix}.k_proj.weight").view(
            length, self.spec.kv_heads, self.spec.head_dim
        )
        value = self._linear(normalized, f"{prefix}.v_proj.weight").view(
            length, self.spec.kv_heads, self.spec.head_dim
        )
        key = key.repeat_interleave(self.spec.query_heads_per_kv_head, dim=1)
        value = value.repeat_interleave(self.spec.query_heads_per_kv_head, dim=1)
        query = query.transpose(0, 1).to(torch.int64)
        key = key.transpose(0, 1).to(torch.int64)
        value = value.transpose(0, 1).to(torch.int64)
        dots = torch.matmul(query, key.transpose(-1, -2))
        scores = round_shift_int(saturate_int32(dots), ATTENTION_SCORE_SHIFT)
        scores = scores + self._alibi_bias(length)

        positions = torch.arange(length, dtype=torch.int64)
        distance = positions[:, None] - positions[None, :]
        valid = ((distance >= 0) & (distance < self.spec.attention_window))[None, :, :]
        maximum = scores.masked_fill(~valid, INT32_MIN).max(dim=-1, keepdim=True).values
        differences = (scores - maximum).clamp(SOFTMAX_MIN_DIFFERENCE, 0)
        attention_weights = self._exp[differences - SOFTMAX_MIN_DIFFERENCE]
        attention_weights = torch.where(
            valid, attention_weights, torch.zeros_like(attention_weights)
        )
        numerator = torch.matmul(attention_weights, value)
        denominator = attention_weights.sum(dim=-1, keepdim=True)
        context = rounded_divide_int(numerator, denominator)
        context = clamp_int8(context).to(torch.int8)
        context = context.transpose(0, 1).reshape(
            length, self.spec.q_heads * self.spec.head_dim
        )
        return self._linear(context, f"{prefix}.out_proj.weight")

    def _ffn(self, normalized: Tensor, prefix: str) -> Tensor:
        projected = self._linear(normalized, f"{prefix}.up_proj.weight")
        activated = torch.clamp_min(projected.to(torch.int32), 0).square()
        return self._linear(activated, f"{prefix}.down_proj.weight")

    @staticmethod
    def _residual(residual: Tensor, update: Tensor) -> Tensor:
        return clamp_int8(residual.to(torch.int16) + update.to(torch.int16)).to(
            torch.int8
        )

    def _forward_hidden(
        self, token_ids: Sequence[int], *, collect_trace: bool
    ) -> tuple[Tensor, tuple[LayerTrace, ...]]:
        tokens = self._validate_tokens(token_ids)
        hidden = self.weights["token_embedding.weight"][tokens]
        layer_traces: list[LayerTrace] = []
        for layer in range(self.spec.layers):
            prefix = f"blocks.{layer}"
            attention = self._attention(self._norm(hidden), f"{prefix}.attention")
            hidden = self._residual(hidden, attention)
            after_attention = tuple(int(value) for value in hidden[-1])
            feed_forward = self._ffn(self._norm(hidden), f"{prefix}.ffn")
            hidden = self._residual(hidden, feed_forward)
            if collect_trace:
                layer_traces.append(
                    LayerTrace(
                        after_attention=after_attention,
                        after_ffn=tuple(int(value) for value in hidden[-1]),
                    )
                )
        return self._norm(hidden), tuple(layer_traces)

    def logits(self, token_ids: Sequence[int]) -> Tensor:
        hidden, _ = self._forward_hidden(token_ids, collect_trace=False)
        return saturate_int32(
            torch.matmul(
                hidden.to(torch.int64),
                self.weights["token_embedding.weight"]
                .to(torch.int64)
                .transpose(-1, -2),
            )
        )

    def greedy_next(self, token_ids: Sequence[int]) -> int:
        return int(self.logits(token_ids)[-1].argmax().item())

    def generate(self, token_ids: Sequence[int], *, max_new_tokens: int) -> list[int]:
        if not isinstance(max_new_tokens, int) or isinstance(max_new_tokens, bool):
            raise TypeError("max_new_tokens must be an integer")
        if max_new_tokens <= 0:
            raise ValueError("max_new_tokens must be positive")
        output = list(token_ids)
        self._validate_tokens(output)
        if len(output) + max_new_tokens - 1 > self.spec.context_length:
            raise ValueError(
                "prefix length + max_new_tokens - 1 exceeds the fixed "
                f"{self.spec.context_length}-token model-input context"
            )
        for _ in range(max_new_tokens):
            next_token = self.greedy_next(output)
            output.append(next_token)
            if next_token == self.spec.eos_token_id:
                break
        return output

    def golden_trace(self, token_ids: Sequence[int]) -> GoldenTrace:
        hidden, layers = self._forward_hidden(token_ids, collect_trace=True)
        logits = saturate_int32(
            torch.matmul(
                hidden[-1].to(torch.int64),
                self.weights["token_embedding.weight"]
                .to(torch.int64)
                .transpose(-1, -2),
            )
        )
        return GoldenTrace(
            architecture_id=ARCHITECTURE_ID,
            input_tokens=tuple(token_ids),
            layers=layers,
            final_hidden=tuple(int(value) for value in hidden[-1]),
            logits=tuple(int(value) for value in logits),
            next_token_id=int(logits.argmax().item()),
        )
