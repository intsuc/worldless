# i64 lowering data pack

This pack compares two scoreboard representations for Java-compatible signed
64-bit addition, subtraction, and comparison without physical-world state.

## Contract

Each public entry point reads `worldless_lab:i64_lowering/input`. The input
must contain exactly these fields:

```snbt
{x:<LongTag>,y:<LongTag>,step:<LongTag>,rounds:<IntTag>}
```

`x`, `y`, and `step` may be any signed 64-bit values. `rounds` must be 1, 8,
or 64. Validation copies the request, removes the four known fields, and
requires the remainder to be empty. The three long fields are classified by
their numeric tag suffix, and `rounds` is round-tripped through a score into an
IntTag and checked by exact NBT partial match. Other numeric tag types are not
accepted through coercion.

The public entry points are:

- `worldless_lab:i64_lowering/two_i32_halves/run`;
- `worldless_lab:i64_lowering/four_u16_limbs/run`.

Invalid input returns failure with value 0 before replacing calculation work
or updating public output. A successful invocation returns success with value
1 and writes this canonical, lossless word representation:

```snbt
{x:[I;<high>,<low>],y:[I;<high>,<low>],less_count:<IntTag>}
```

For each value, `high = (value >> 32) as i32` and `low = value as i32`. The
lab clears output before every checked invocation, so its checked output is
exactly the compound above. General reconstruction of an arbitrary LongTag
from two command-result scores would introduce a separate conversion
algorithm, so output remains in canonical words.

## Workload

One logical round performs these operations in order:

```text
if signed_i64(x) < signed_i64(y):
    less_count += 1
x = wrapping_i64(x + y)
y = wrapping_i64(y - step)
```

This is the Java SE 25 `long` contract: values are signed two's-complement
64-bit integers, addition and subtraction discard overflow beyond 64 bits,
and relational comparison is signed. The Rust suite owns the oracle using
`i64::wrapping_add`, `i64::wrapping_sub`, and signed `<`.

Four operand profiles are each run for 1, 8, and 64 rounds:

| Profile | `x` | `y` | `step` | Boundary emphasis |
| --- | ---: | ---: | ---: | --- |
| `low_order` | 1,311,768,464,867,721,232 | 1,311,768,469,162,688,496 | 33 | Equal high words, unsigned-low order, repeated add carry |
| `add_wrap` | 9,223,372,036,854,775,800 | 16 | 4,294,967,297 | Signed maximum crossing and word carry |
| `sub_wrap` | -9,223,372,036,854,775,800 | -9,223,372,036,854,775,801 | 16 | Signed wrap, equal-high comparisons, and borrow |
| `mixed` | 7,640,891,576,956,012,809 | -4,942,790,177,534,073,029 | 4,354,685,564,936,845,355 | Dense carries, borrows, and sign changes |

The one-round rows expose fixed LongTag ABI and representation-conversion
cost. The longer rows amortize that boundary and measure repeated arithmetic.
Drivers statically repeat one shared step resource; there is no runtime loop,
dynamic index, recursive call, or per-round macro expansion.

## LongTag input boundary

Both lowerings first use the same exact LongTag splitter. The low word is
obtained by assigning the LongTag into one IntArray element, which performs
Minecraft's numeric narrowing, and then reading that IntTag into a score. The
high word is estimated with scales `+2^-32` and `-2^-32`. A shared correction
tree handles double-rounding boundaries for negative low words. This is the
same algorithm covered by Worldless's numeric-conversion edge tests.

Splitting, validation, dispatch, representation conversion, all rounds,
canonical output conversion, and publication are inside each timed public
call. Input installation, output clearing, result inspection, pack loading,
and parsing are outside it.

## Lowerings

`two_i32_halves` keeps each long as a signed high word and a permanently
sign-biased low word. Adding `i32::MIN` maps unsigned 32-bit order onto signed
score order, so ordinary score comparison detects low-word order, carry, and
borrow. Addition and subtraction apply the same bias once more to keep the
result in that representation. The lowering keeps six carrier scores for
`x`, `y`, and `step` and converts the two result lows back only at the ABI.

`four_u16_limbs` keeps each long in four scores from least to most significant,
each in `0..=65535`. Its top limb remains sign-biased by 32,768, so signed
comparison is an ordinary lexicographic limb comparison without per-round
conversion. Addition propagates carries with division and modulo by 65,536
and corrects the biased top; subtraction uses Minecraft's floor division and
modulo, where a negative limb difference yields borrow `-1` and a normalized
remainder. It keeps twelve carrier scores.

Both representations preserve the complete intermediate `x` and `y` values
through every round and convert them to the same two-word ABI only at the end.
The experiment does not cover multiplication, division, remainder, shifts,
bitwise operations, conversion back to LongTag, more than three live long
values, or storage spills. Results must not be extended to those operations.

## Cost model and pack size

The two-half lowering has 10 begin commands, 16 step commands, and 6 end
commands. The four-limb lowering has 32, 46, and 14 respectively. Thus, for
`R` rounds, the four-limb path executes `30R + 30` more variant source commands
before accounting for condition-chain evaluation. Both paths use the same
number of driver and common-function calls.

Worldless quota is also affected by which prefix of a compound `execute`
comparison is evaluated. Consequently, exact quota varies by operand profile,
and the measured table below is authoritative for the selected inputs. Source
commands, score-carrier count, quota, and elapsed time are separate axes.

The dynamic part is still checkable. Before each round, let `A` mean that the
top limbs are equal, `H` that the complete high 32-bit words are equal, and `C`
that the upper three limbs are equal. For a profile-dependent fixed boundary
term `F`, Worldless reports:

```text
Qtwo  = F + 22R + sum(H)
Qfour = F + 30 + 55R + sum(A + H + C)
Qfour - Qtwo = 30 + 33R + sum(A + C)
```

All quota cells below match these identities. The terms count executed
condition prefixes as well as function and arithmetic commands; they are not
source-line counts.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is 24,193
bytes, with 21 function resources and 360 nonblank command lines. The longest
line is 229 bytes. The portions are:

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, split, dispatch, accumulation, and output | 8,178 | 7 | 82 |
| `two_i32_halves` | 5,799 | 7 | 109 |
| `four_u16_limbs` | 10,109 | 7 | 169 |

`pack.mcmeta` accounts for the remaining 107 bytes. Each variant portion
includes its public wrapper, begin/step/end resources, and three static
drivers. Pack loading and parsing are outside both timing protocols.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite i64_lowering --format text
cargo run --release -p worldless-lab -- compare --suite i64_lowering \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Profile | Rounds | Two i32 halves `quota; median` | Four u16 limbs `quota; median` |
| --- | ---: | ---: | ---: |
| `low_order` | 1 | 170; 36.850 | 234; 55.960 |
| `low_order` | 8 | 324; 75.211 | 619; 153.800 |
| `low_order` | 64 | 1,556; 371.861 | 3,699; 932.282 |
| `add_wrap` | 1 | 185; 37.740 | 248; 56.191 |
| `add_wrap` | 8 | 339; 75.650 | 633; 154.570 |
| `add_wrap` | 64 | 1,571; 383.251 | 3,713; 825.032 |
| `sub_wrap` | 1 | 162; 27.680 | 227; 43.840 |
| `sub_wrap` | 8 | 318; 57.611 | 618; 120.661 |
| `sub_wrap` | 64 | 1,578; 292.911 | 3,782; 754.073 |
| `mixed` | 1 | 233; 30.180 | 296; 44.560 |
| `mixed` | 8 | 387; 59.030 | 681; 120.490 |
| `mixed` | 64 | 1,619; 285.051 | 3,761; 722.612 |

The two-half lowering used less quota and had a lower median in all 12 rows.
At 64 rounds, it used 57.0 to 58.3 percent less quota and had a 53.5 to 61.2
percent lower median than four limbs, depending on the operand profile. The
profile-dependent spread reflects both input splitting and lexicographic
comparison depth; it does not change the ranking in this matrix.

The one-round rows include the complete input and output boundary. Two halves
already had a 32.3 to 36.9 percent lower median there because four limbs must
expand three input values to twelve scores and collapse two results back to
words. Reuse widened the absolute difference, while both implementations
retained exact Java-compatible results across signed and word boundaries.

Worldless timing alone does not establish the same ratio for Minecraft Java
Edition. Runtime implementation, batching, and row-order controls differ, so
comparisons should remain within one runtime.

### Minecraft Java Edition

The same 12 inputs and both public entry points were checked and measured on
Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31. The host was
the same Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The server used
Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players.
The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `B` consecutive public calls. Calibration selected a power-of-two `B`, up
to 1,024, toward 100 ms while keeping `B` times the Worldless quota below
800,000. Each of two fresh JVMs discarded 20 warm-up batches and measured 15
batches per row; the second JVM traversed all rows in reverse order. The table
pools the 30 unfiltered batch averages. Each cell is
`B; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight required success with value 1. Preflight and every
post-batch check required exact public output and exact internal `work.result`,
both without extra fields. Input installation, output clearing, calibration,
preflight, and result inspection were outside the stopwatch. Each timed call
still included exact validation, LongTag splitting, representation conversion,
all rounds, ABI conversion, and publication.

The two measured JVMs recorded 109 and 123 young collections, no full
collections, and maximum pauses of 23.916 and 19.914 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Profile | Rounds | Two i32 halves `B; median / p95` | Four u16 limbs `B; median / p95` |
| --- | ---: | ---: | ---: |
| `low_order` | 1 | 1,024; 18.555 / 19.531 | 1,024; 26.367 / 30.273 |
| `low_order` | 8 | 1,024; 34.668 / 35.156 | 1,024; 59.082 / 64.453 |
| `low_order` | 64 | 512; 149.414 / 152.344 | 128; 324.219 / 351.562 |
| `add_wrap` | 1 | 1,024; 19.531 / 21.484 | 1,024; 26.367 / 27.344 |
| `add_wrap` | 8 | 1,024; 36.133 / 37.109 | 1,024; 59.570 / 64.453 |
| `add_wrap` | 64 | 256; 156.250 / 164.062 | 128; 332.031 / 359.375 |
| `sub_wrap` | 1 | 1,024; 18.555 / 20.508 | 1,024; 25.879 / 27.344 |
| `sub_wrap` | 8 | 1,024; 34.180 / 34.180 | 1,024; 60.059 / 64.453 |
| `sub_wrap` | 64 | 256; 152.344 / 160.156 | 128; 343.750 / 375.000 |
| `mixed` | 1 | 1,024; 21.484 / 21.484 | 1,024; 27.344 / 28.320 |
| `mixed` | 8 | 1,024; 35.156 / 36.133 | 1,024; 59.570 / 63.477 |
| `mixed` | 64 | 256; 144.531 / 171.875 | 128; 339.844 / 359.375 |

The two-half lowering had a lower median in all 12 rows. At 64 rounds, its
median was 52.9 to 57.5 percent lower than four limbs, while its quota was
57.0 to 58.3 percent lower. Even the one-round rows, which include the complete
LongTag and result ABI boundary, had 21.4 to 29.6 percent lower medians because
four limbs expand three inputs to twelve carriers and collapse two outputs.

Minecraft and Worldless agree on the qualitative result: keeping two biased
32-bit words was cheaper throughout this bounded add/subtract/compare matrix.
Their absolute times and ratios are not paired latencies because Minecraft
measures server-JVM batches with a 1 ms clock and Worldless measures individual
calls with its host clock.

## Failure boundaries

Disposable Worldless checks covered every missing field, ordinary and
maximum-depth extra fields, wrong LongTag and rounds types, unsupported round
counts, and arbitrary inputs containing `i64::MIN`, `i64::MAX`, `-1`, `0`, and
low-word boundaries. Invalid calls returned failure without changing
pre-existing work or public output.
All valid boundary inputs produced the independently calculated words and
comparison count in both lowerings.

Command-limit interruption is not transactional. For each variant, a direct
64-round invocation with a limit equal to its measured successful quota
reported `CommandLimitExceeded`; one additional command succeeded. At limit
100, the old public output remained, and a subsequent full-limit call on the
same VM succeeded because `prepare` replaced all calculation state. Execution
status, rather than apparently complete internal or public data, is
authoritative at a quota boundary.

A separate fresh Minecraft JVM ran the maximum-quota
`sub_wrap`/64-round/four-limb row with an empirical command-sequence limit of
100. The server reported stopping after exactly 100 commands; the surrounding
`execute store` retained its return sentinel of -999, public output was absent,
and `work.result` was still the empty compound created during preparation.
Restoring the normal limit of 1,000,000 and rerunning the full preflight on the
same JVM produced the exact expected result. This probe was excluded from
calibration, timing samples, and the measured JVMs' GC results.

## Compiler implications

For add/subtract/compare-heavy signed 64-bit code on this target, a compiler
should prefer two i32 words over four u16 limbs when both are otherwise legal.
The low-word sign bias gives an inexpensive reusable unsigned-order mapping,
while high-word carry and borrow remain ordinary wrapping score operations.
It also halves the persistent carrier-score footprint and produces less
generated source.

The four-limb representation remains a valid bounded lowering and avoids
unsigned 32-bit values inside each carrier, but that property did not repay
its extra propagation and comparison work here. It may have different
tradeoffs for multiplication, division, shifts, bitwise operations, or spill
layouts; none of those are measured. A compiler still needs one canonical
word-pair ABI and explicit conversion points so independently compiled code
does not silently mix representations.
