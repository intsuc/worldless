from __future__ import annotations

import math
from typing import Literal

import torch
import torch.nn.functional as F
from torch import Tensor, nn

from .quantization import (
    RuntimeState,
    clamp_int8,
    exact_integer_matmul,
    fake_quantize_int8,
    quantize_int8,
    requantize_int8,
    round_half_away,
    round_shift_int,
    rounded_divide_int,
    saturate_int32,
    ste,
)
from .spec import (
    ATTENTION_SCORE_SHIFT,
    DEFAULT_DENSE_SHIFTS,
    INT32_MIN,
    KNOWN_MODEL_SPECS,
    KNOWN_MODEL_WIDTHS,
    RMS_GAIN_FRACTION_BITS,
    RMS_GAIN_TABLE,
    RMS_TARGET,
    SOFTMAX_MIN_DIFFERENCE,
    TRAINING_LOGIT_SHIFT,
    ModelSpec,
    exp_q15_table,
    expected_weight_shapes,
    zero_shift_weight_names,
)

ExecutionMode = Literal["float", "fake_runtime"]


def _validate_mode(mode: str) -> ExecutionMode:
    if mode not in ("float", "fake_runtime"):
        raise ValueError("mode must be 'float' or 'fake_runtime'")
    return mode


class AffineFreeRMSNorm(nn.Module):
    def __init__(self, width: int) -> None:
        super().__init__()
        if width not in KNOWN_MODEL_WIDTHS:
            raise ValueError(f"RMSNorm width must be one of {KNOWN_MODEL_WIDTHS}")
        self.width = width
        self.register_buffer(
            "_gain_table",
            torch.tensor(RMS_GAIN_TABLE, dtype=torch.int64),
            persistent=False,
        )

    def forward(self, inputs: Tensor, *, mode: ExecutionMode) -> Tensor:
        if mode == "float":
            mean_square = inputs.square().mean(dim=-1, keepdim=True).clamp_min(1.0)
            return inputs * (RMS_TARGET * torch.rsqrt(mean_square))

        quantized = quantize_int8(inputs).to(torch.int64)
        mean_square = torch.div(
            quantized.square().sum(dim=-1, keepdim=True),
            self.width,
            rounding_mode="floor",
        )
        gain = self._gain_table[mean_square]
        exact = rounded_divide_int(
            quantized * gain,
            torch.full_like(gain, 1 << RMS_GAIN_FRACTION_BITS),
        )
        exact = clamp_int8(exact).to(torch.int8)
        surrogate_mean = inputs.square().mean(dim=-1, keepdim=True).clamp_min(1.0)
        surrogate = inputs * (RMS_TARGET * torch.rsqrt(surrogate_mean))
        return ste(exact, surrogate)


class RuntimeLinear(nn.Module):
    def __init__(
        self,
        in_features: int,
        out_features: int,
        *,
        shift: int,
        int8_input: bool = True,
        initialization_scale: float = 1.0,
    ) -> None:
        super().__init__()
        self.int8_input = int8_input
        self.weight = nn.Parameter(torch.empty(out_features, in_features))
        self.register_buffer("shift", torch.tensor(shift, dtype=torch.int32))
        input_scale = 1.0 if int8_input else RMS_TARGET
        standard_deviation = (
            initialization_scale * (2**shift) / (input_scale * math.sqrt(in_features))
        )
        nn.init.normal_(self.weight, mean=0.0, std=standard_deviation)
        with torch.no_grad():
            self.weight.clamp_(-127, 127)

    @property
    def shift_exponent(self) -> int:
        return int(self.shift.item())

    def forward(self, inputs: Tensor, *, mode: ExecutionMode) -> Tensor:
        shift = self.shift_exponent
        if mode == "float":
            return F.linear(inputs, self.weight) / (2**shift)

        quantized_weight = quantize_int8(self.weight)
        if self.int8_input:
            quantized_input = quantize_int8(inputs).to(torch.int64)
        else:
            quantized_input = round_half_away(inputs).to(torch.int64)
        accumulator = exact_integer_matmul(
            quantized_input, quantized_weight.transpose(-1, -2)
        )
        accumulator = saturate_int32(accumulator)
        exact = requantize_int8(accumulator, shift)

        fake_weight = fake_quantize_int8(self.weight)
        surrogate = F.linear(inputs, fake_weight) / (2**shift)
        return ste(exact, surrogate)


def _relative_distances(length: int, device: torch.device) -> Tensor:
    positions = torch.arange(length, device=device, dtype=torch.int64)
    return positions[:, None] - positions[None, :]


def _valid_attention_mask(length: int, window: int, device: torch.device) -> Tensor:
    distances = _relative_distances(length, device)
    return (distances >= 0) & (distances < window)


def _float_alibi_bias(
    length: int,
    device: torch.device,
    dtype: torch.dtype,
    slopes: tuple[tuple[int, int], ...],
) -> Tensor:
    distance = _relative_distances(length, device).to(dtype)
    slopes = torch.tensor(
        [numerator / denominator for numerator, denominator in slopes],
        device=device,
        dtype=dtype,
    )
    return -slopes[:, None, None] * distance[None, :, :]


def _integer_alibi_bias(
    length: int,
    device: torch.device,
    logit_denominator: int,
    slopes: tuple[tuple[int, int], ...],
) -> Tensor:
    distance = _relative_distances(length, device)
    per_head: list[Tensor] = []
    for numerator, denominator in slopes:
        bias_numerator = -logit_denominator * numerator * distance
        per_head.append(
            rounded_divide_int(
                bias_numerator,
                torch.full_like(bias_numerator, denominator),
            )
        )
    return torch.stack(per_head)


class MultiQueryAttention(nn.Module):
    def __init__(
        self,
        spec: ModelSpec,
        *,
        layer_index: int,
        logit_denominator: int,
    ) -> None:
        super().__init__()
        self.spec = spec
        self.logit_denominator = logit_denominator
        self.q_proj = RuntimeLinear(
            spec.d_model,
            spec.q_heads * spec.head_dim,
            shift=DEFAULT_DENSE_SHIFTS["q_proj"],
        )
        self.k_proj = RuntimeLinear(
            spec.d_model,
            spec.kv_heads * spec.head_dim,
            shift=DEFAULT_DENSE_SHIFTS["k_proj"],
        )
        self.v_proj = RuntimeLinear(
            spec.d_model,
            spec.kv_heads * spec.head_dim,
            shift=DEFAULT_DENSE_SHIFTS["v_proj"],
        )
        self.value_embedding: nn.Embedding | None
        if layer_index in spec.value_embedding_layers:
            self.value_embedding = nn.Embedding(
                spec.vocab_size, spec.kv_heads * spec.head_dim
            )
            nn.init.normal_(self.value_embedding.weight, mean=0.0, std=1.0)
        else:
            self.value_embedding = None
        self.out_proj = RuntimeLinear(
            spec.q_heads * spec.head_dim,
            spec.d_model,
            shift=DEFAULT_DENSE_SHIFTS["out_proj"],
            initialization_scale=1 / math.sqrt(2 * spec.layers),
        )
        self.register_buffer(
            "_exp_table",
            torch.tensor(exp_q15_table(logit_denominator), dtype=torch.int64),
            persistent=False,
        )

    def _reshape_queries(self, value: Tensor) -> Tensor:
        batch, length, _ = value.shape
        return value.view(batch, length, self.spec.q_heads, self.spec.head_dim)

    def _reshape_kv(self, value: Tensor) -> Tensor:
        batch, length, _ = value.shape
        return value.view(batch, length, self.spec.kv_heads, self.spec.head_dim)

    def _expand_kv_heads(self, value: Tensor) -> Tensor:
        if self.spec.kv_heads == self.spec.q_heads:
            return value
        return value.repeat_interleave(self.spec.query_heads_per_kv_head, dim=2)

    def _float_attention(self, query: Tensor, key: Tensor, value: Tensor) -> Tensor:
        length = query.shape[1]
        query_heads = query.transpose(1, 2)
        key_heads = self._expand_kv_heads(key).transpose(1, 2)
        value_heads = self._expand_kv_heads(value).transpose(1, 2)
        scores = torch.matmul(query_heads, key_heads.transpose(-1, -2))
        scores = scores / ((2**ATTENTION_SCORE_SHIFT) * self.logit_denominator)
        scores = (
            scores
            + _float_alibi_bias(
                length, scores.device, scores.dtype, self.spec.alibi_slopes
            )[None, :, :, :]
        )
        valid = _valid_attention_mask(length, self.spec.attention_window, scores.device)
        scores = scores.masked_fill(~valid[None, None, :, :], -torch.inf)
        probabilities = torch.softmax(scores, dim=-1)
        context = torch.matmul(probabilities, value_heads)
        return context.transpose(1, 2).reshape(
            query.shape[0], length, self.spec.q_heads * self.spec.head_dim
        )

    def _exact_attention(self, query: Tensor, key: Tensor, value: Tensor) -> Tensor:
        length = query.shape[1]
        query_heads = quantize_int8(query).to(torch.int64).transpose(1, 2)
        key_heads = (
            self._expand_kv_heads(quantize_int8(key)).to(torch.int64).transpose(1, 2)
        )
        value_heads = (
            self._expand_kv_heads(quantize_int8(value)).to(torch.int64).transpose(1, 2)
        )
        dots = exact_integer_matmul(query_heads, key_heads.transpose(-1, -2))
        scores = round_shift_int(saturate_int32(dots), ATTENTION_SCORE_SHIFT)
        scores = (
            scores
            + _integer_alibi_bias(
                length,
                scores.device,
                self.logit_denominator,
                self.spec.alibi_slopes,
            )[None, :, :, :]
        )
        valid = _valid_attention_mask(
            length, self.spec.attention_window, scores.device
        )[None, None, :, :]
        masked_scores = scores.masked_fill(~valid, INT32_MIN)
        maximum = masked_scores.max(dim=-1, keepdim=True).values
        difference = (scores - maximum).clamp(SOFTMAX_MIN_DIFFERENCE, 0)
        weights = self._exp_table[difference - SOFTMAX_MIN_DIFFERENCE]
        weights = torch.where(valid, weights, torch.zeros_like(weights))
        numerator = exact_integer_matmul(weights, value_heads)
        denominator = weights.sum(dim=-1, keepdim=True)
        context = rounded_divide_int(numerator, denominator)
        context = clamp_int8(context).to(torch.int8)
        return context.transpose(1, 2).reshape(
            query.shape[0], length, self.spec.q_heads * self.spec.head_dim
        )

    def _project_value(
        self, inputs: Tensor, token_ids: Tensor, *, mode: ExecutionMode
    ) -> Tensor:
        projected = self.v_proj(inputs, mode=mode)
        if self.value_embedding is None:
            return projected
        if mode == "float":
            return projected + self.value_embedding(token_ids)

        quantized_embedding = quantize_int8(self.value_embedding.weight)
        embedded = quantized_embedding[token_ids]
        exact = clamp_int8(
            quantize_int8(projected).to(torch.int32) + embedded.to(torch.int32)
        ).to(torch.int8)
        surrogate = (
            projected + fake_quantize_int8(self.value_embedding.weight)[token_ids]
        )
        return ste(exact, surrogate)

    def forward(
        self, inputs: Tensor, token_ids: Tensor, *, mode: ExecutionMode
    ) -> Tensor:
        query = self._reshape_queries(self.q_proj(inputs, mode=mode))
        key = self._reshape_kv(self.k_proj(inputs, mode=mode))
        value = self._reshape_kv(self._project_value(inputs, token_ids, mode=mode))
        if mode == "float":
            context = self._float_attention(query, key, value)
        else:
            exact = self._exact_attention(query, key, value)
            surrogate = self._float_attention(query, key, value)
            context = ste(exact, surrogate)
        return self.out_proj(context, mode=mode)


class ReluSquaredFFN(nn.Module):
    def __init__(self, spec: ModelSpec) -> None:
        super().__init__()
        self.up_proj = RuntimeLinear(
            spec.d_model,
            spec.d_ff,
            shift=DEFAULT_DENSE_SHIFTS["up_proj"],
        )
        self.down_proj = RuntimeLinear(
            spec.d_ff,
            spec.d_model,
            shift=DEFAULT_DENSE_SHIFTS["down_proj"],
            int8_input=False,
            initialization_scale=1 / math.sqrt(2 * spec.layers),
        )

    def forward(self, inputs: Tensor, *, mode: ExecutionMode) -> Tensor:
        projected = self.up_proj(inputs, mode=mode)
        if mode == "float":
            activated = F.relu(projected).square()
        else:
            quantized = quantize_int8(projected).to(torch.int32)
            exact = torch.clamp_min(quantized, 0).square()
            surrogate = F.relu(projected).square()
            activated = ste(exact, surrogate)
        return self.down_proj(activated, mode=mode)


def _residual_add(residual: Tensor, update: Tensor, mode: ExecutionMode) -> Tensor:
    summed = residual + update
    if mode == "float":
        return summed
    return ste(quantize_int8(summed), summed)


class TransformerBlock(nn.Module):
    def __init__(
        self,
        spec: ModelSpec,
        *,
        layer_index: int,
        attention_logit_denominator: int,
    ) -> None:
        super().__init__()
        self.attn_norm = AffineFreeRMSNorm(spec.d_model)
        self.attention = MultiQueryAttention(
            spec,
            layer_index=layer_index,
            logit_denominator=attention_logit_denominator,
        )
        self.ffn_norm = AffineFreeRMSNorm(spec.d_model)
        self.ffn = ReluSquaredFFN(spec)

    def forward(
        self, inputs: Tensor, token_ids: Tensor, *, mode: ExecutionMode
    ) -> Tensor:
        attention = self.attention(
            self.attn_norm(inputs, mode=mode), token_ids, mode=mode
        )
        hidden = _residual_add(inputs, attention, mode)
        feed_forward = self.ffn(self.ffn_norm(hidden, mode=mode), mode=mode)
        return _residual_add(hidden, feed_forward, mode)


class Transformer(nn.Module):
    def __init__(
        self,
        spec: ModelSpec,
        *,
        attention_logit_denominator: int | None = None,
    ) -> None:
        super().__init__()
        if spec not in KNOWN_MODEL_SPECS:
            raise ValueError("spec must match a known architecture")
        resolved_logit_denominator = (
            spec.runtime_attention_logit_denominator
            if attention_logit_denominator is None
            else attention_logit_denominator
        )
        exp_q15_table(resolved_logit_denominator)
        self.spec = spec
        self.attention_logit_denominator = resolved_logit_denominator
        self.token_embedding = nn.Embedding(spec.vocab_size, spec.d_model)
        nn.init.normal_(self.token_embedding.weight, mean=0.0, std=2.0)
        self.blocks = nn.ModuleList(
            TransformerBlock(
                spec,
                layer_index=layer_index,
                attention_logit_denominator=resolved_logit_denominator,
            )
            for layer_index in range(spec.layers)
        )
        self.final_norm = AffineFreeRMSNorm(spec.d_model)
        self.lm_head: nn.Linear | None
        if spec.tied_lm_head:
            self.lm_head = None
        else:
            self.lm_head = nn.Linear(spec.d_model, spec.vocab_size, bias=False)
            nn.init.normal_(self.lm_head.weight, mean=0.0, std=2.0)

    def require_runtime_compatible(self) -> None:
        if not self.spec.data_pack_runtime_compatible:
            raise ValueError(
                f"architecture {self.spec.architecture_id!r} is not supported by "
                "the data-pack runtime"
            )
        required_denominator = self.spec.runtime_attention_logit_denominator
        if self.attention_logit_denominator != required_denominator:
            raise ValueError(
                "checkpoint attention logit denominator "
                f"{self.attention_logit_denominator} does not match architecture "
                f"runtime denominator {required_denominator}"
            )

    def _validate_tokens(self, token_ids: Tensor) -> None:
        if token_ids.dtype not in (torch.int32, torch.int64):
            raise TypeError("token_ids must have dtype torch.int32 or torch.int64")
        if token_ids.ndim != 2:
            raise ValueError("token_ids must have shape [batch, sequence]")
        if not 0 < token_ids.shape[1] <= self.spec.context_length:
            raise ValueError(
                f"sequence length must be in 1..{self.spec.context_length}"
            )
        if torch.any(token_ids < 0) or torch.any(token_ids >= self.spec.vocab_size):
            raise ValueError(f"token IDs must be in 0..{self.spec.vocab_size - 1}")

    def forward(
        self,
        token_ids: Tensor,
        *,
        mode: ExecutionMode = "float",
        raw_logits: bool = False,
    ) -> Tensor:
        mode = _validate_mode(mode)
        self._validate_tokens(token_ids)
        if mode == "float":
            hidden = self.token_embedding(token_ids)
            embedding_weight = self.token_embedding.weight
        else:
            embedding_weight = fake_quantize_int8(self.token_embedding.weight)
            hidden = F.embedding(token_ids, embedding_weight)
        for block in self.blocks:
            hidden = block(hidden, token_ids, mode=mode)
        hidden = self.final_norm(hidden, mode=mode)

        output_weight = (
            embedding_weight
            if self.lm_head is None
            else (
                self.lm_head.weight
                if mode == "float"
                else fake_quantize_int8(self.lm_head.weight)
            )
        )

        if mode == "float":
            logits = F.linear(hidden, output_weight)
        else:
            exact = saturate_int32(
                exact_integer_matmul(
                    quantize_int8(hidden),
                    quantize_int8(output_weight).transpose(-1, -2),
                )
            )
            surrogate = F.linear(hidden, output_weight)
            logits = ste(exact, surrogate)
        if not raw_logits:
            logits = logits / (2**TRAINING_LOGIT_SHIFT)
        return logits

    @torch.no_grad()
    def generate(
        self,
        prefix: Tensor,
        *,
        max_new_tokens: int,
        mode: ExecutionMode = "fake_runtime",
    ) -> Tensor:
        self._validate_tokens(prefix)
        if prefix.shape[0] != 1:
            raise ValueError("generate requires a single prefix")
        if not isinstance(max_new_tokens, int) or isinstance(max_new_tokens, bool):
            raise TypeError("max_new_tokens must be an integer")
        if max_new_tokens <= 0:
            raise ValueError("max_new_tokens must be positive")
        if prefix.shape[1] + max_new_tokens - 1 > self.spec.context_length:
            raise ValueError(
                "prefix length + max_new_tokens - 1 exceeds the fixed "
                f"{self.spec.context_length}-token model-input context"
            )
        output = prefix.clone()
        for _ in range(max_new_tokens):
            logits = self(output, mode=mode, raw_logits=True)
            next_token = logits[:, -1, :].argmax(dim=-1, keepdim=True)
            output = torch.cat((output, next_token), dim=1)
            if int(next_token.item()) == self.spec.eos_token_id:
                break
        return output

    def runtime_state(self) -> RuntimeState:
        for name, parameter in self.named_parameters():
            if not bool(torch.isfinite(parameter).all()):
                raise ValueError(f"runtime parameter {name!r} contains NaN or infinity")
        weights: dict[str, Tensor] = {
            "token_embedding.weight": quantize_int8(
                self.token_embedding.weight.detach()
            )
            .cpu()
            .contiguous()
        }
        shifts: dict[str, int] = {"token_embedding.weight": 0}
        for layer_index in self.spec.value_embedding_layers:
            value_embedding = self.blocks[layer_index].attention.value_embedding
            if value_embedding is None:
                raise RuntimeError("architecture value embedding is missing")
            key = f"blocks.{layer_index}.attention.value_embedding.weight"
            weights[key] = (
                quantize_int8(value_embedding.weight.detach()).cpu().contiguous()
            )
            shifts[key] = 0
        if self.lm_head is not None:
            weights["lm_head.weight"] = (
                quantize_int8(self.lm_head.weight.detach()).cpu().contiguous()
            )
            shifts["lm_head.weight"] = 0
        for module_name, module in self.named_modules():
            if isinstance(module, RuntimeLinear):
                key = f"{module_name}.weight"
                weights[key] = quantize_int8(module.weight.detach()).cpu().contiguous()
                shifts[key] = module.shift_exponent
        expected = expected_weight_shapes(self.spec)
        if set(weights) != set(expected):
            raise RuntimeError(
                "model runtime tensor keys do not match architecture schema"
            )
        for key, shape in expected.items():
            if tuple(weights[key].shape) != shape:
                raise RuntimeError(
                    f"runtime tensor {key!r} has shape {tuple(weights[key].shape)}, "
                    f"expected {shape}"
                )
        for key in zero_shift_weight_names(self.spec):
            if shifts[key] != 0:
                raise RuntimeError(f"runtime tensor {key!r} must use shift zero")
        return RuntimeState(weights=weights, shifts=shifts)

    def parameter_count(self) -> int:
        return sum(parameter.numel() for parameter in self.parameters())
