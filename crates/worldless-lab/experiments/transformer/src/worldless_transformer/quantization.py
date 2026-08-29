from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass

import torch
from torch import Tensor

from .spec import (
    INT8_MAX,
    INT8_MIN,
    INT32_MAX,
    INT32_MIN,
    REQUANT_SHIFT_MAX,
    REQUANT_SHIFT_MIN,
)


def round_half_away(value: Tensor) -> Tensor:
    return torch.sign(value) * torch.floor(torch.abs(value) + 0.5)


def clamp_int8(value: Tensor) -> Tensor:
    return value.clamp(INT8_MIN, INT8_MAX)


def quantize_int8(value: Tensor) -> Tensor:
    return clamp_int8(round_half_away(value)).to(torch.int8)


def saturate_int32(value: Tensor) -> Tensor:
    return value.clamp(INT32_MIN, INT32_MAX).to(torch.int32)


def exact_integer_matmul(left: Tensor, right: Tensor) -> Tensor:
    if left.is_floating_point() or right.is_floating_point():
        raise TypeError("exact_integer_matmul operands must have integer dtypes")
    # CUDA has no int64 matmul. Every fixed-ABI accumulator is below 2**53, so
    # binary64 multiply-adds preserve each integer exactly on both CPU and CUDA.
    return torch.matmul(left.to(torch.float64), right.to(torch.float64)).to(torch.int64)


def round_shift_int(value: Tensor, shift: int) -> Tensor:
    if not REQUANT_SHIFT_MIN <= shift <= REQUANT_SHIFT_MAX:
        raise ValueError(
            f"requant shift must be in {REQUANT_SHIFT_MIN}..{REQUANT_SHIFT_MAX}"
        )
    value = value.to(torch.int64)
    if shift == 0:
        return value
    half = 1 << (shift - 1)
    adjusted = value + torch.sign(value) * half
    return torch.div(adjusted, 1 << shift, rounding_mode="trunc")


def requantize_int8(value: Tensor, shift: int) -> Tensor:
    return clamp_int8(round_shift_int(value, shift)).to(torch.int8)


def rounded_divide_int(numerator: Tensor, denominator: Tensor) -> Tensor:
    if torch.any(denominator <= 0):
        raise ValueError("rounded integer division requires a positive denominator")
    numerator = numerator.to(torch.int64)
    denominator = denominator.to(torch.int64)
    adjusted = numerator + torch.sign(numerator) * torch.div(
        denominator, 2, rounding_mode="floor"
    )
    return torch.div(adjusted, denominator, rounding_mode="trunc")


def ste(exact: Tensor, surrogate: Tensor) -> Tensor:
    exact_float = exact.to(dtype=surrogate.dtype)
    return surrogate + (exact_float - surrogate).detach()


def fake_quantize_int8(value: Tensor) -> Tensor:
    return ste(quantize_int8(value), value)


@dataclass(frozen=True, slots=True)
class RuntimeState:
    weights: Mapping[str, Tensor]
    shifts: Mapping[str, int]

    def __post_init__(self) -> None:
        if set(self.weights) != set(self.shifts):
            missing_shifts = sorted(set(self.weights) - set(self.shifts))
            missing_weights = sorted(set(self.shifts) - set(self.weights))
            raise ValueError(
                "runtime state key mismatch: "
                f"missing_shifts={missing_shifts}, missing_weights={missing_weights}"
            )
        for key, weight in self.weights.items():
            if weight.dtype != torch.int8:
                raise ValueError(f"runtime weight {key!r} must have dtype torch.int8")
            if torch.any(weight < INT8_MIN) or torch.any(weight > INT8_MAX):
                raise ValueError(
                    f"runtime weight {key!r} must contain only {INT8_MIN}..{INT8_MAX}"
                )
            shift = self.shifts[key]
            if not isinstance(shift, int) or isinstance(shift, bool):
                raise TypeError(f"runtime shift for {key!r} must be an integer")
            if not REQUANT_SHIFT_MIN <= shift <= REQUANT_SHIFT_MAX:
                raise ValueError(
                    f"runtime shift for {key!r} must be in "
                    f"{REQUANT_SHIFT_MIN}..{REQUANT_SHIFT_MAX}"
                )
