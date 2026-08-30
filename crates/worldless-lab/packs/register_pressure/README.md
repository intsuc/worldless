# Register pressure data pack

This pack compares four ways to preserve homogeneous caller-local integer
values across nested, reentrant function calls without physical-world state.
It fixes call depth and root count, then varies only the number of values that
remain live across each child call.

## Contract

Each public entry point reads `worldless_lab:register_pressure/input`. The
input must contain exactly these fields:

```snbt
{width:<IntTag>,seeds:[I;...exactly 15 integers...]}
```

`width` must be one of `1`, `2`, `4`, `8`, or `16`. Each seed may be any
signed 32-bit integer. The pack round-trips the width through a score into an
IntTag and checks the seed value as an IntArrayTag before inspecting its
length. Missing or extra fields, other tag types, other widths, and other seed
counts are rejected rather than coerced.

Validation uses separate scratch state. Invalid input returns failure with
value 0 before replacing calculation work, changing its calculation scores,
or updating public output. Every valid invocation replaces all three spill
stacks before it starts.

The public entry points are:

- `worldless_lab:register_pressure/static_scores/run`;
- `worldless_lab:register_pressure/word_stack/run`;
- `worldless_lab:register_pressure/compound_stack/run`;
- `worldless_lab:register_pressure/hot_4_spill/run`.

A successful invocation returns success with value 1 and writes these known
fields:

```snbt
{width:<IntTag>,checksum:<IntTag>}
```

Before publication, the common finish function requires all three spill
stacks to be present and empty, all 15 roots to have completed, exactly 120
activations to have run, and exactly `120 * width` locals to have been folded.
The lab clears output storage to `{}` before every invocation, so its checked
output is exactly the compound above. Data-pack commands cannot replace a
storage root compound; callers that do not clear it retain unrelated
pre-existing fields.

## Workload

The recursion depth is fixed at 8. For signed 32-bit wrapping arithmetic, one
activation of `F(n, x, W)` is:

```text
for j in 0 .. W:
  local[j] = x * 31 + n + (j + 1)

if n == 1:
  child = x * 17 + 7
else:
  child = F(n - 1, x + local[0], W)

result = child
for j in 0 .. W:
  result = result * 31 + local[j]
return result
```

All `W` locals are defined before the child call and read in ascending order
after it returns. The recursive edge is an ordinary function call because the
caller still has restoration and folding work to do. The driver calls
`F(8, seed, W)` for all 15 seeds and folds the roots into a checksum starting
at 1:

```text
checksum = checksum * 31 + result
```

The Rust suite owns one fixed seed array for every width. It includes minimum,
zero, and maximum signed integers as well as mixed positive and negative
values. Thus every row executes 120 activations, 105 nested caller-save edges,
and `120 * W` post-call local folds.

The timed public call includes exact validation, work initialization, one
warmed function-macro dispatch, all 15 recursive roots, completion checks, and
publication. Input installation, output clearing, result inspection, pack
compilation, and VM construction are outside the timer.

## Preservation strategies

- `static_scores` assigns every local a depth-specific score holder. It does
  not spill to storage, but generates separate level functions for every
  supported width.
- `word_stack` uses `W` shared scratch scores and appends each caller's `W`
  locals to one flat IntArray. Unwinding restores the words in reverse stack
  order before folding them in ascending local order.
- `compound_stack` appends one compound frame with `W` named integer fields,
  stores and reloads every field, then removes the frame. It also uses `W`
  shared scratch scores.
- `hot_4_spill` gives the first `min(W, 4)` locals depth-specific score
  holders and spills only the remaining `max(W - 4, 0)` locals to a flat
  IntArray. The four hot holders are per static depth, not four holders shared
  by all activations.

For widths no greater than 4, `hot_4_spill` and `static_scores` execute the
same command shape and use exactly the same quota. The fixed control skeleton
also remains the same across strategies: every activation generates and folds
the same locals, performs the same base test, and increments the same
counters. Only caller-local preservation differs.

At width 16 and depth 8, the representation footprints are:

| Strategy | Local score holders | Peak spill storage |
| --- | ---: | ---: |
| Static scores | 128 | none |
| Word stack | 16 | 112 integer words |
| Compound stack | 16 | 7 frames of 16 integer fields |
| Hot 4 + spill | 44 | 84 integer words |

## Cost model and pack size

For the supported widths, measured Worldless quota follows these exact
formulas:

```text
static_scores  = 1056 + 840W
word_stack     = 1056 + 1470W
compound_stack = 1266 + 1260W
hot_4_spill    = 1056 + 840W + 630 max(W - 4, 0)
```

There are 105 nested edges. Relative to static scores, one flat saved word
adds six commands per edge: append, store, reload, and remove. One compound
frame adds `4W + 2` commands per edge: one append and remove plus a store and
reload for every field. The hybrid pays the flat-word cost only for locals
beyond its four hot slots.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is
456,323 bytes, with 135 function resources and 6,497 nonblank command lines.
The longest command line is 156 bytes. The following footprint counts include
each variant's public wrapper, five width drivers, and implementation
resources. Shared dynamic base functions serve both storage-only strategies.

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, dispatch, fold, and finish | 5,798 | 6 | 68 |
| Shared dynamic bases | 7,208 | 5 | 113 |
| `static_scores` | 153,382 | 51 | 2,285 |
| `word_stack` | 49,901 | 11 | 675 |
| `compound_stack` | 47,302 | 11 | 623 |
| `hot_4_spill` | 192,624 | 51 | 2,733 |

`pack.mcmeta` accounts for the remaining 108 bytes. Pack loading, parsing, and
parsed-command memory are outside both timing protocols, so generated source
and score-holder count are required second axes rather than costs already
represented by elapsed time.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite register_pressure --format text
cargo run --release -p worldless-lab -- compare --suite register_pressure \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Width | Static scores `quota; median` | Word stack `quota; median` | Compound stack `quota; median` | Hot 4 + spill `quota; median` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1,896; 451.901 | 2,526; 544.322 | 2,526; 562.712 | 1,896; 377.231 |
| 2 | 2,736; 551.822 | 3,996; 703.182 | 3,786; 687.582 | 2,736; 556.611 |
| 4 | 4,416; 964.314 | 6,936; 1,242.694 | 6,306; 1,190.314 | 4,416; 959.953 |
| 8 | 7,776; 1,824.836 | 12,816; 2,360.878 | 11,346; 2,231.258 | 10,296; 2,108.467 |
| 16 | 14,496; 3,654.722 | 24,576; 4,578.226 | 21,426; 4,279.355 | 22,056; 4,372.295 |

The no-spill static and hybrid forms tied for lowest quota through width 4.
Their width-2 and width-4 medians were within 0.9 percent; the larger width-1
difference illustrates the noise and fixed row-order sensitivity of short
rows. Static scores had the lowest quota and median at widths 8 and 16, but
width 16 requires 128 local holders and the static portion alone is about 153
KB. At width 8, retaining four hot locals reduced the median by 5.5 percent
relative to a compound-only stack. At width 16, the twelve flat spills made
the hybrid 2.2 percent slower than the compound stack. The compound stack was
also 6.5 percent faster than the all-word stack there.

The measured crossover therefore lies somewhere between the tested widths 8
and 16 for this runtime and workload. The matrix does not include width 5, so
it does not identify the first profitable spill or an exact threshold.

### Minecraft Java Edition

The same five inputs and all four public entry points were checked and measured
on Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31. The host
was the same Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The server used
Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players.
The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` complete public calls. Calibration selected a power-of-two `R`, up to
1,024, toward 100 ms while keeping `R` times the Worldless quota below
800,000. The quota cap left several median batches below the accepted band's
50 ms lower bound. Each of two fresh JVMs discarded 20 warm-up batches and
measured 15 batches per row; the second JVM traversed all rows in reverse
order. The table pools the 30 unfiltered batch averages. Each cell is
`R; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight and post-batch check required return success and
value 1, exact public and internal result compounds, all three spill stacks
present and empty, `roots == 15`, `activations == 120`, and
`folds == 120 * width`. Input installation, state poisoning, checks, and score
retrieval were outside the stopwatch. Calibration used the same stopwatch
batches but was excluded from the 30 reported samples.

The two measured JVMs recorded 158 and 146 young collections, no full
collections, and maximum pauses of 49.810 and 24.052 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Width | Static scores `R; median / p95` | Word stack `R; median / p95` | Compound stack `R; median / p95` | Hot 4 + spill `R; median / p95` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 256; 167.969 / 199.219 | 256; 261.719 / 289.063 | 256; 271.484 / 281.250 | 256; 167.969 / 179.688 |
| 2 | 256; 242.188 / 250.000 | 128; 429.688 / 437.500 | 128; 398.438 / 414.063 | 256; 242.188 / 250.000 |
| 4 | 128; 382.813 / 398.438 | 64; 734.375 / 750.000 | 64; 640.625 / 671.875 | 128; 382.813 / 398.438 |
| 8 | 64; 710.938 / 750.000 | 32; 1,375.000 / 1,406.250 | 64; 1,125.000 / 1,156.250 | 64; 1,093.750 / 1,140.625 |
| 16 | 32; 1,406.250 / 1,468.750 | 32; 2,625.000 / 2,687.500 | 32; 2,125.000 / 2,187.500 | 32; 2,562.500 / 2,812.500 |

Static and hybrid medians were equal through width 4, their no-spill
calibration range. At width 8, the hybrid was 2.8 percent faster than the
compound stack. At width 16, the compound stack was 17.1 percent faster than
the hybrid and 19.0 percent faster than the all-word stack. Minecraft thus
showed the same qualitative crossover as Worldless, but penalized twelve
independent flat spills much more strongly.

Static scores remained fastest, including a 33.8 percent advantage over the
compound stack at width 16. That is the bounded-specialization option, not a
general unbounded-frame implementation: its runtime result must be weighed
against 128 local holders and substantially more generated source.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock; Worldless measures individual calls with its host clock.
Runtime implementation, batching, GC, and order controls differ, so
comparisons should remain within each runtime.

## Failure boundaries

Disposable Worldless and Minecraft checks each covered 18 invalid shapes for
every variant. Across the two matrices, these included either or both fields
missing, an extra field, wrong width tags, wrong seed container tags, seed
lengths 14 and 16, and rejected widths -1, 0, 3, 5, and 17. All 72 invocations
per runtime returned failure with value 0 while distinct sentinels in public
output, calculation work, and the calculation scores remained unchanged.
Arrays containing fifteen minimum, zero, or maximum signed IntTags also
produced the independent oracle result for all variants.

For each width-16 variant, a Worldless invocation with its exact measured
quota `Q` reported `CommandLimitExceeded`, while `Q + 1` succeeded with exact
state and output. The respective `Q` values were 14,496, 24,576, 21,426, and
22,056 for static, word, compound, and hybrid preservation.

At limit 512, public output retained its old sentinel while calculation work
was partial. A separate Minecraft JVM observed six static activations, 59
saved word-stack values, four compound frames, or 48 hybrid overflow words at
the interruption point. The outer return consumer also retained its sentinel.
Restoring the normal limit and clearing public output allowed every variant to
pass a complete preflight on the same VM or server because valid preparation
replaces all spill stacks and calculation scores.

Function execution and publication are not transactional. Command-limit
status is authoritative even if some internal state or output appears
complete near a boundary; a generated language runtime must reset or stage
state explicitly after interruption.

## Compiler implications and limits

For this workload, bounded score-only preservation is the fastest region when
generated source and a depth-times-width score bank are acceptable. With four
hot locals and four spills, hybrid preservation is useful at width 8. Once the
overflow reaches twelve words, a single compound frame is the better tested
spill representation. A compiler can use these as initial heuristic regions,
but should not encode an exact threshold without measuring the missing widths
and its own generated call body.

The experiment fixes depth 8, fifteen roots, homogeneous i32 locals, and the
five listed widths. It does not cover heterogeneous values, argument
marshalling, aggregate or multiple returns, tail calls, dynamic depth,
concurrency, or heap-owned objects. The benchmark deliberately preserves and
folds every canonical local independently; its recurrence is algebraically
compressible, so common-subexpression elimination and live-set compression
are separate compiler passes rather than costs measured here. Data-pack load
latency, parsed-command memory, and runtime frame memory are also unmeasured.
