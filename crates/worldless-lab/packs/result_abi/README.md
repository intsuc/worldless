# Result ABI data pack

This pack compares four ways to return a fixed-width tuple of homogeneous
signed integers from a sequential leaf function without physical-world state.
It fixes the producer body and call count, then varies only the channel used to
materialize and transfer the result.

## Contract

Each public entry point reads `worldless_lab:result_abi/input`. The input must
contain exactly these fields:

```snbt
{width:<IntTag>,seed:<IntTag>}
```

`width` must be one of `1`, `2`, `4`, `8`, or `16`. `seed` may be any signed
32-bit integer. Both values are round-tripped through scores into IntTags
before calculation state is replaced. Missing or extra fields, other tag
types, and other widths are rejected rather than coerced.

Validation uses separate scratch state. Invalid input returns failure with
value 0 before replacing calculation work, changing calculation scores, or
updating public output. Every valid invocation replaces its frame list and
result channel before starting.

The public entry points are:

- `worldless_lab:result_abi/score_slots/run`;
- `worldless_lab:result_abi/return_head/run`;
- `worldless_lab:result_abi/caller_out/run`;
- `worldless_lab:result_abi/callee_frame/run`.

A successful invocation returns success with value 1 and writes these known
fields:

```snbt
{width:<IntTag>,checksum:<IntTag>}
```

Before publication, the common finish function requires the caller frame list
to be present and empty, all 31 producers to have completed, and exactly
`31 * width` components to have been both emitted and folded. The lab clears
output storage to `{}` before every invocation, so its checked output is
exactly the compound above. Data-pack commands cannot replace a storage root
compound; callers that do not clear it retain unrelated pre-existing fields.

## Workload

The generator state starts at `seed`, and the checksum starts at 1. Each of 31
sequential producer calls emits one tuple. For signed 32-bit wrapping
arithmetic, a call and its immediate consumer are:

```text
for j in 0 .. width:
  state = state * 1664525 + 1013904223
  result[j] = state

for j in 0 .. width:
  checksum = checksum * 31 + result[j]
```

The generator continues across calls, so the result stream is the first
`31 * width` successors of one LCG. Every component depends on the previous
one and is materialized before the state advances again. The caller consumes
all components in ascending order immediately after each producer returns.

The Rust suite supplies seed `-123456789` for every registered width and owns
an independent wrapping-i32 oracle. The driver statically emits the 31 call
sites. There is no runtime loop, recursive call, dynamic NBT path, or macro in
the measured producer/consumer path. One warmed function macro selects the
variant and width before entering that path.

The timed public call includes exact validation, channel initialization,
dispatch, all 31 producer calls, tuple consumption, completion checks, and
publication. Input installation, output clearing, result inspection, pack
compilation, and VM construction are outside the timer.

## Result channels

- `score_slots` emits every component into a fixed score holder `#r0` through
  `#r15`. The caller folds those holders directly.
- `return_head` emits component zero into a private `#head` holder and returns
  it through the function command value. The caller captures that value into
  `#r0`; components one and above use the same score holders and consumer as
  `score_slots`.
- `caller_out` appends one zero-filled, caller-owned compound frame before
  each call. The callee writes the width-specific fields from `v0` through
  `v(width - 1)` into the top frame, and the caller loads, folds, and removes
  it.
- `callee_frame` lets the callee overwrite a fixed singleton compound. After
  return, the caller append-copies that completed compound into the same
  frame representation consumed by `caller_out`.

The storage strategies intentionally materialize a fresh caller-owned frame
for every aggregate result, even though the caller consumes it immediately.
Frame allocation and ownership transfer are therefore part of the measured
ABI. `caller_out` and `callee_frame` execute the same number of commands, but
they do not execute identical NBT operations: one writes through the dynamic
top-frame path, while the other writes a fixed singleton and copies it.

All strategies use the same generator scores, arithmetic order, one explicit
emission per component, producer count, fold order, and completion counters.
This extends the scalar-result comparison in `call_abi`; argument transport,
macro-cache behavior, and caller-local preservation are fixed or absent here.

## Cost model and pack size

For the five supported widths, measured Worldless quota follows these exact
formulas:

```text
score_slots = 165 + 217W
return_head = 227 + 217W
caller_out  = 227 + 310W
callee_frame = 227 + 310W
```

The 217-per-width slope is seven commands for each of 31 components: two LCG
operations, one score emission, one emitted-value count, two checksum
operations, and one fold count. Returning the head adds exactly two commands
per producer, or 62 per invocation. A compound component adds three commands
relative to a score component: the storage emission and reload need three
more quota units in total. Appending and removing one caller frame adds two
more per producer. Thus each storage strategy adds exactly
`31 * (3W + 2)` to `score_slots`.

At width 16, the active result-channel footprints are 16 result score holders
for `score_slots`, one private head plus 16 caller-visible result holders for
`return_head`, and two width-16 compounds for either storage strategy: the
template or singleton plus at most one caller-owned frame. Infrastructure and
validation scores are shared and excluded from those counts.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is
150,545 bytes, with 58 function resources and 2,385 nonblank command lines.
The longest command line is 156 bytes. The portions below separate the shared
consumers from each variant's public wrapper, five drivers, and five producer
resources.

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, dispatch, and finish | 4,533 | 4 | 56 |
| Shared score consumers | 6,082 | 5 | 93 |
| Shared frame consumers | 9,602 | 5 | 129 |
| `score_slots` | 21,713 | 11 | 448 |
| `return_head` | 29,108 | 11 | 453 |
| `caller_out` | 39,325 | 11 | 603 |
| `callee_frame` | 40,081 | 11 | 603 |

`pack.mcmeta` accounts for the remaining 101 bytes. Pack loading, parsing, and
parsed-command memory are outside both timing protocols, so source size and
channel footprint are separate compiler tradeoffs rather than costs already
represented by elapsed time.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite result_abi --format text
cargo run --release -p worldless-lab -- compare --suite result_abi \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Width | Score slots `quota; median` | Return head `quota; median` | Caller out `quota; median` | Callee frame `quota; median` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 382; 92.140 | 444; 100.360 | 537; 126.801 | 537; 127.670 |
| 2 | 599; 151.960 | 661; 161.381 | 847; 201.860 | 847; 189.671 |
| 4 | 1,033; 239.501 | 1,095; 221.441 | 1,467; 270.811 | 1,467; 273.211 |
| 8 | 1,901; 396.001 | 1,963; 403.352 | 2,707; 508.121 | 2,707; 509.192 |
| 16 | 3,637; 785.862 | 3,699; 789.193 | 5,187; 975.473 | 5,187; 974.403 |

Score slots used the least quota at every width. The return channel's fixed
62-command delta shrank from 16.2 percent of score-slot quota at width 1 to
1.7 percent at width 16. Their width-16 medians differed by 0.4 percent. The
width-4 elapsed ordering inverted despite the higher return quota, so these
fixed-order medians do not support a stronger claim about a small runtime
difference between the two score-based channels.

The storage channels used equal quota at every width. Their medians did not
show a consistent winner: the width-2 callee-frame row was lower, while the
other four pairs were within 0.9 percent. At width 16, compound transport used
42.6 percent more quota and about 24 percent more median time than score
slots.

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
value 1, exact public and internal result compounds, an empty caller frame
list, exact channel and dispatch layouts, `calls == 31`,
`values == folds == 31 * width`, and the oracle's generator state. Channels
that retain the final result tuple after consumption were checked against it
as well. Input installation, state poisoning, checks, and score retrieval were
outside the stopwatch. Calibration used the same stopwatch batches but was
excluded from the 30 reported samples.

The two measured JVMs recorded 157 and 152 young collections, no full
collections, and maximum pauses of 24.957 and 25.245 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Width | Score slots `R; median / p95` | Return head `R; median / p95` | Caller out `R; median / p95` | Callee frame `R; median / p95` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1,024; 32.227 / 40.039 | 1,024; 40.039 / 48.828 | 1,024; 64.453 / 65.430 | 1,024; 64.453 / 66.406 |
| 2 | 1,024; 52.734 / 54.688 | 1,024; 60.547 / 61.523 | 512; 97.656 / 101.563 | 512; 97.656 / 99.609 |
| 4 | 512; 89.844 / 91.797 | 512; 96.680 / 101.563 | 512; 160.156 / 164.063 | 512; 160.156 / 164.063 |
| 8 | 256; 160.156 / 167.969 | 256; 167.969 / 175.781 | 256; 285.156 / 292.969 | 256; 285.156 / 292.969 |
| 16 | 128; 304.688 / 312.500 | 128; 312.500 / 328.125 | 128; 546.875 / 570.313 | 128; 546.875 / 617.188 |

Score slots had the lowest Minecraft median at every width. Returning the head
through the native command value increased the median by 24.2 percent at
width 1, but its fixed cost amortized to 2.6 percent at width 16. It did not
produce a measured throughput advantage over a dedicated score lane in this
sequential contract.

Caller-out and callee-frame medians were equal at all five widths at the
stopwatch's resolution, matching their equal command quota. This result does
not prove the underlying NBT operations have equal cost; their p95 values and
paths differ, and the benchmark balances a dynamic top-frame destination
against a fixed destination followed by a compound copy. At width 16, either
storage strategy was 79.5 percent slower than score slots.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock; Worldless measures individual calls with its host clock.
Runtime implementation, batching, GC, and order controls differ, so
comparisons should remain within each runtime.

## Failure boundaries

Disposable Worldless and Minecraft checks each covered 24 invalid shapes for
every variant. Across the matrix, these included either or both fields
missing, an extra field, six wrong numeric or string tags for `width`, nine
wrong scalar or container tags for `seed`, and rejected widths -1, 0, 3, 5,
and 17. All 96 invocations per runtime returned failure with value 0 while
distinct sentinels in public output, calculation work, and calculation scores
remained unchanged. Width-16 inputs using minimum, zero, or maximum signed
integer seeds also produced the independent oracle result for every variant
in both runtimes.

For each width-16 variant, a Worldless invocation with its exact measured
quota `Q` reported `CommandLimitExceeded`, while `Q + 1` succeeded with exact
state and output. The respective `Q` values were 3,637, 3,699, 5,187, and
5,187 for score slots, returned head, caller out, and callee frame.

At limit 512, public output retained its old sentinel while calculation work
was partial. A separate Minecraft JVM reached four score-based producers or
three storage-based producers. Each storage variant retained one complete,
not-yet-removed caller frame while only 40 of its 48 generated components had
been folded. The outer return consumer also retained its sentinel. Restoring
the normal limit and clearing public output allowed every variant to pass a
complete preflight on the same VM or server because a valid invocation resets
work and core scores, then overwrites every active carrier before reading it.

Function execution and publication are not transactional. Command-limit
status is authoritative even if some internal state or output appears complete
near a boundary; a generated language runtime must reset or stage state
explicitly after interruption.

## Compiler implications and limits

For a fixed homogeneous tuple that is consumed immediately, dedicated result
scores have the lowest command quota of the tested lowerings. Using the native
command result for only the head does not remove the remaining score bank and
adds a fixed capture/return cost. It may express a different ownership
convention, but this measurement did not establish a consistent
sustained-throughput advantage for it.

A compound result costs more, but gives the caller an aggregate frame rather
than a set of global result registers. Direct caller-out and callee-owned-copy
forms had the same quota and unresolved median difference in this workload,
so a compiler should choose between them based on ownership and opportunities
to forward an existing destination. This experiment deliberately allocates a
new caller-owned frame for every result; retaining a borrowed callee singleton
or eliding the copy into a pre-existing destination is a different ABI.

The experiment fixes 31 sequential calls, immediate full consumption,
homogeneous i32 results, and the five statically specialized widths. It does
not cover nested or reentrant calls, concurrent invocations, heterogeneous
NBT results, dynamic width, partial-use dead-code elimination, forwarding or
copy elision, arbitrary caller-selected destinations, recursive result
propagation, macro or CPS dispatch, heap ownership, or aggregate arguments.
Data-pack load latency, parsed-command memory, and runtime carrier memory are
also unmeasured.
