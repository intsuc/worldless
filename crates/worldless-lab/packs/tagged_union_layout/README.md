# Tagged union layout data pack

This pack compares four NBT layouts for a small tagged union of homogeneous
signed integers. It fixes tag loading, candidate generation, construction,
tag decoding, arm dispatch, payload reads, and checksum work, then varies only
the physical representation of one short-lived union cell.

## Contract

Each public entry point reads `worldless_lab:tagged_union_layout/input`. The
input must contain exactly these fields:

```snbt
{tags:[I; <exactly 31 values>],seed:<IntTag>}
```

Every tag must be an integer from 0 through 3. `seed` may be any signed 32-bit
integer. Missing or extra fields, another NBT type, another array length, and
tags outside the supported range are rejected rather than coerced.

Validation uses separate scratch storage and score holders. Invalid input
returns failure with value 0 before replacing calculation work, changing the
generator, candidate, checksum, or completion scores, or updating public
output. Validation scratch, validation scores, and input-loading scores may
change during a rejected call. A valid call copies the 31 tags into fixed
holders `#input0` through `#input30`, replaces the private work cell and
result, initializes the generator, and sets the prepared gate before entering
the driver.

The public entry points are:

- `worldless_lab:tagged_union_layout/narrow_compound/run`;
- `worldless_lab:tagged_union_layout/wide_compound/run`;
- `worldless_lab:tagged_union_layout/narrow_array/run`;
- `worldless_lab:tagged_union_layout/wide_array/run`.

A successful invocation returns success with value 1 and publishes:

```snbt
{checksum:<IntTag>}
```

The common finish function requires the evaluation count to equal the
validated tag count of 31 before publishing. The private storage is exactly a
`work` compound containing the final `cell` and
`result:{checksum:<IntTag>}`. The lab clears output storage to `{}` before
each call, so its checked output is exactly the compound above. Data-pack
commands cannot replace a storage root compound; callers that do not clear it
retain unrelated pre-existing fields.

## Logical union and oracle

The four logical arms deliberately give two different tags the same payload
width:

| Tag | Logical arm | Active payload width |
| ---: | --- | ---: |
| 0 | `PairEarly` | 2 |
| 1 | `None` | 0 |
| 2 | `Scalar` | 1 |
| 3 | `PairLate` | 2 |

The generator state starts at `seed`, and the checksum starts at 1. Every one
of the 31 rounds generates both candidates before dispatching on the current
tag. For signed wrapping-i32 arithmetic, the complete round is:

```text
p0 = state
state = state * 1664525 + 1013904223
p1 = state
state = state * 1664525 + 1013904223

construct cell(tag, active prefix of [p0, p1])
decoded_tag = read cell.tag
active_payload = read payload selected by decoded_tag

arm = decoded_tag + 1
for p in active_payload:
  arm = arm * 31 + p
checksum = checksum * 31 + arm
```

Construction and matching therefore cross the physical NBT representation;
the checksum is not folded directly from `#source_tag`, `#p0`, or `#p1`.
The matcher decodes the tag from the cell and reloads every active payload
component from that cell.

All registered cases use seed `-123456789`. The two mixed traces have the
same tag histogram, total active width, quota, final tag, and final physical
cell preconditions, while changing the order of the tags.

| Case | Tag trace | Active-width sum | Expected checksum |
| --- | --- | ---: | ---: |
| `pair_early` | tag 0 repeated 31 times | 62 | 1,529,901,803 |
| `none` | tag 1 repeated 31 times | 0 | 1,291,863,073 |
| `scalar` | tag 2 repeated 31 times | 31 | 1,572,944,173 |
| `pair_late` | tag 3 repeated 31 times | 62 | -1,803,522,930 |
| `cycle_4` | `(0,1,2,3)` seven times, then `0,1,2` | 38 | 90,226,150 |
| `clustered_4` | `0` eight, `1` eight, `3` seven, `2` eight | 38 | 241,466,934 |

The Rust suite owns an independent implementation of this oracle. Both LCG
successors are generated even for `None` and `Scalar`, so payload width
changes only construction and matching work, not the candidate stream.

## Physical layouts

Both compound variants use a nested payload compound. The narrow compound
stores only the active fields:

```snbt
tag 0: {tag:0,payload:{p0:<IntTag>,p1:<IntTag>}}
tag 1: {tag:1,payload:{}}
tag 2: {tag:2,payload:{p0:<IntTag>}}
tag 3: {tag:3,payload:{p0:<IntTag>,p1:<IntTag>}}
```

The wide compound always allocates
`{tag:<IntTag>,payload:{p0:<IntTag>,p1:<IntTag>}}`. Its zero-width arm leaves
both payload fields at zero, and its one-width arm writes only `p0` while
leaving `p1` at zero.

The narrow array stores the tag at index 0 followed by exactly the active
payload prefix:

```snbt
tag 0: [I;0,p0,p1]
tag 1: [I;1]
tag 2: [I;2,p0]
tag 3: [I;3,p0,p1]
```

The wide array is always `[I;tag,p0,p1]`. As with the wide compound, inactive
slots come from the zero-filled allocation literal and are never written from
the generated candidates. Thus stale values cannot leak from one arm to the
next.

The narrow and wide representations are physically identical for both pair
arms. Those rows act as controls for the padding comparison: tag 0 and tag 3
always activate both available payload slots. Compound and IntArray tag and
payload paths remain different, but each container family shares one matcher
and its three width-specific handlers between its narrow and wide variants.

## Common execution path

Each layout has a statically unrolled 31-step driver. Every step performs the
same sequence:

1. copy the validated tag into `#source_tag`;
2. generate both score candidates;
3. dispatch through four tag conditions and one active constructor;
4. replace the complete cell with one allocation command and store only the
   active payload components;
5. decode the tag from the Compound or IntArray;
6. dispatch through four decoded-tag conditions and one active handler;
7. reload the active payload, calculate the arm fold, update the checksum,
   and increment the evaluation count.

There is no runtime loop, recursion, dynamic NBT path, or payload mutation or
writeback after construction. Function macros are used only for exact input
validation and are warm in the persistent measurement. The public call
includes validation, tag loading, work reset, all 31 steps, completion checks,
and publication. Input installation, output clearing, result inspection,
data-pack loading, and VM construction are outside the timer.

Allocation literal length changes between narrow arms, but it remains one
command. All variants issue the same four constructor conditions and four
matcher conditions. Each active payload component adds the same one storage
write, one storage read, and checksum arithmetic regardless of container
family. Equal same-case command quota is therefore a required fairness
invariant, not an observed ranking of the layouts.

## Cost model and pack size

For Worldless, the exact quota for every layout is:

```text
width = [2, 0, 1, 2]
Q = 173 + sum over 31 tags of (29 + 6 * width[tag])
```

The fixed 173 commands cover common validation, setup, public dispatch, and
finish work outside the repeated steps. The 29-command base covers candidate
generation, function boundaries, four-way constructor and matcher dispatch,
cell allocation, tag decoding, and completion counting. Each active payload
component adds two quota units for the stored score value, two for reloading
it, and two for the arm multiply-add.

This gives quota 1,444 for either pair case, 1,072 for `none`, 1,258 for
`scalar`, and 1,300 for either mixed case. All four layouts have exactly the
same quota in every case.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is
37,575 bytes across 46 files: 45 function resources and `pack.mcmeta`. It
contains 523 nonblank command lines, and the longest command is 133 bytes.
The variant portions below contain the public wrapper, driver, step,
constructor dispatcher, and four constructors; family matchers and handlers
are listed separately because narrow and wide share them.

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, generation, and finish | 11,590 | 5 | 139 |
| Shared Compound matcher and handlers | 1,998 | 4 | 26 |
| Shared IntArray matcher and handlers | 1,961 | 4 | 26 |
| `narrow_compound` variant portion | 5,620 | 8 | 83 |
| `wide_compound` variant portion | 5,560 | 8 | 83 |
| `narrow_array` variant portion | 5,402 | 8 | 83 |
| `wide_array` variant portion | 5,334 | 8 | 83 |

`pack.mcmeta` accounts for the remaining 110 bytes. Pack loading, command
parsing, and parsed-resource memory are outside both timing protocols. Cell
construction is timed, but the memory footprint of the resulting cell is not
measured separately. The small source-byte differences between narrow and
wide portions also include resource-path and slug spelling, including the
shorter word `wide`; they are not evidence of a semantic code-size win.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite tagged_union_layout --format text
cargo run --release -p worldless-lab -- compare --suite tagged_union_layout \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. It used one
persistent VM per row, one discarded warm-up invocation per row, 31 measured
invocations, and a warm validation-macro cache. Each cell is `quota; median`,
with median time in microseconds. Times are environment-dependent, and
Worldless traverses rows in fixed table order.

| Case | Narrow Compound `quota; median` | Wide Compound `quota; median` | Narrow IntArray `quota; median` | Wide IntArray `quota; median` |
| --- | ---: | ---: | ---: | ---: |
| `pair_early` | 1,444; 310.051 | 1,444; 308.800 | 1,444; 302.601 | 1,444; 295.771 |
| `none` | 1,072; 218.971 | 1,072; 222.601 | 1,072; 215.980 | 1,072; 216.461 |
| `scalar` | 1,258; 264.821 | 1,258; 252.990 | 1,258; 219.681 | 1,258; 200.851 |
| `pair_late` | 1,444; 232.451 | 1,444; 234.911 | 1,444; 223.591 | 1,444; 223.411 |
| `cycle_4` | 1,300; 210.241 | 1,300; 212.101 | 1,300; 206.061 | 1,300; 202.671 |
| `clustered_4` | 1,300; 211.641 | 1,300; 213.830 | 1,300; 203.461 | 1,300; 204.891 |

For every case, both IntArray medians were below their corresponding Compound
median. Across the 12 matched narrow or wide comparisons, the reduction
ranged from 1.4 to 20.6 percent in this run. Equal quota rules out a lower
Worldless command quota as the explanation, but it does not isolate the
elapsed difference as a container-operation cost. The fixed row order and
especially the wider spread in the `scalar` rows further limit that
interpretation.

Omitting inactive padding had no consistent median advantage. Narrow was
lower in six of the 12 within-family comparisons and wide was lower in the
other six. This includes the pair controls, where narrow and wide construct
the same physical shape.

For a given layout, `cycle_4` and `clustered_4` medians differed by at most
1.3 percent, with the direction changing across variants. These rows do not
establish a benefit for cycling or clustering the tags. Conversely,
`pair_late` was 23.9 to 26.1 percent lower than `pair_early` despite identical
payload width and corresponding physical shapes. The Minecraft results below
do not reproduce that magnitude or a common direction, so this fixed-order
Worldless difference is not evidence for a portable tag-number or arm-order
optimization.

### Minecraft Java Edition

The same six traces and all four public entry points were checked and measured
on Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31. The host
was the same AMD Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The runtime
was Microsoft OpenJDK 25.0.1+8-LTS. The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` complete public calls. JVM A calibrated a power-of-two `R`, up to
1,024, toward 100 ms and within a preferred 50 to 250 ms range while requiring
`R * Worldless quota < 800,000`; JVM B reused that row's `R`. Calibration
selected 512 for all 24 rows. Each of two fresh measurement JVMs discarded 20
warm-up batches and measured 15 batches per row. JVM A traversed the complete
matrix forward and JVM B traversed it in reverse. The table pools all 30
unfiltered batch averages, and each cell is
`R; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight and post-batch check required return success with
value 1, exact public and internal result compounds, the exact nested
Compound or IntArray cell without extra fields, and the expected generator,
candidates, decoded tag, active payload, arm, checksum, input scores, and
completion count. The two mixed rows also had their equal final-cell
preconditions checked. Input installation, state poisoning, single-call
preflight, post-batch validation, and score retrieval were outside
the reported measurement boundary. Calibration batches used the same
stopwatch, but are excluded from the 30 pooled samples. The timed boundary was
`R` consecutive complete public calls; only the final call was wrapped to
capture result and success before reading the stopwatch.

The two measured JVMs recorded 192 and 197 young collections, no full
collections, and maximum pauses of 25.081 and 49.255 ms. Samples were not
filtered, so pauses inside timed batches remain in the results; the 49.255 ms
maximum in JVM B occurred in a discarded warm-up batch.

| Case | Narrow Compound `R; median / p95` | Wide Compound `R; median / p95` | Narrow IntArray `R; median / p95` | Wide IntArray `R; median / p95` |
| --- | ---: | ---: | ---: | ---: |
| `pair_early` | 512; 142.578 / 148.438 | 512; 142.578 / 144.531 | 512; 132.813 / 138.672 | 512; 131.836 / 138.672 |
| `none` | 512; 100.586 / 105.469 | 512; 103.516 / 107.422 | 512; 97.656 / 101.563 | 512; 97.656 / 99.609 |
| `scalar` | 512; 118.164 / 125.000 | 512; 119.141 / 125.000 | 512; 114.258 / 117.188 | 512; 114.258 / 119.141 |
| `pair_late` | 512; 138.672 / 144.531 | 512; 138.672 / 144.531 | 512; 133.789 / 138.672 | 512; 132.813 / 138.672 |
| `cycle_4` | 512; 125.000 / 128.906 | 512; 125.977 / 130.859 | 512; 121.094 / 123.047 | 512; 119.141 / 123.047 |
| `clustered_4` | 512; 125.000 / 130.859 | 512; 125.000 / 130.859 | 512; 120.117 / 138.672 | 512; 120.117 / 138.672 |

Minecraft also placed each IntArray median below its matching Compound median.
The reductions across the 12 comparisons ranged from 2.9 to 7.5 percent. The
padding comparison remained unresolved: narrow was lower in three rows, wide
was lower in three, and six were tied at the stopwatch's resolution.

The `cycle_4` and `clustered_4` medians differed by at most 0.8 percent for a
given layout, again without one sequence winning throughout. For the two pair
tags, `pair_late` was 2.7 percent lower in both Compound layouts but 0.7
percent higher in both IntArray layouts. This does not reproduce the sharp
Worldless separation and does not support a portable tag-order claim.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock; Worldless measures individual calls with its host clock.
Runtime implementation, batching, GC, fixed versus reversed order controls,
and timing resolution differ. Timing comparisons should therefore remain
within one runtime.

## Failure boundaries

Disposable Worldless and Minecraft checks each covered 23 invalid shapes for
all four variants, or 92 invocations per runtime. The matrix included missing
or extra fields; scalar, List, Compound, String, ByteArray, and LongArray
shapes in place of the tags IntArray; lengths 0, 30, and 32; tag values -1 and
4 at both the first and last positions; and Byte, Short, Long, Float, Double,
and String shapes in place of the seed IntTag. Every invocation returned
failure with value 0 while distinct public-output, calculation-work, and core
calculation-score sentinels remained unchanged. Validation scratch and its
input-loading state are intentionally not part of that preservation contract
and may change.

Separate success probes used seeds -2,147,483,648, 0, and 2,147,483,647 with
the `cycle_4` trace for all four variants. All 12 invocations per runtime
matched the independent wrapping-i32 oracle and the exact final physical cell.

For every one of the 24 Worldless rows, an invocation with its exact measured
quota `Q` reported `CommandLimitExceeded`. `Q + 1` then succeeded with exact
state and output on the same VM. The tested `Q` values were 1,444 for either
pair case, 1,072 for `none`, 1,258 for `scalar`, and 1,300 for either mixed
case.

At limit 512, the `cycle_4` row for every layout stopped after nine completed
evaluations in both Worldless and a separate Minecraft probe JVM. The
prepared gate and partial cell were present, while the old public-output
sentinel remained unchanged. Restoring the normal limit, clearing only public
output, and rerunning the valid public invocation on the same VM or server
succeeded for all four layouts without an external calculation-state reset.
The invocation itself replaces the cell, result, generator, checksum, and
completion state before reading them. These recovery probes were separate
from the timed samples.

Function execution and publication are not transactional. Command-limit
status is authoritative even when internal work resembles a valid prefix; a
generated language runtime must reset or stage state explicitly after an
interruption.

## Compiler implications and limits

For this bounded homogeneous union, IntArray storage had a lower median than
the corresponding nested Compound in every recorded row in both runtimes,
while consuming exactly the same command quota. A compiler targeting fixed
integer payloads and fixed paths can therefore consider an IntArray carrier,
subject to the loss of named fields and the absence of heterogeneous payload
types.

The measurements do not show a consistent throughput benefit from omitting
inactive padding. Wide layouts deliberately leave inactive slots at zero and
never store generated candidates into them. Narrow layouts express the active
shape more precisely, while wide layouts provide fixed offsets. Since runtime
cell memory was not measured, this experiment cannot turn the source shape
or timing result into a density or memory-footprint claim.

The experiment replaces one singleton cell each round and retains only the
final cell after matching. It is not a collection-density benchmark and does
not measure arrays of unions, several simultaneously live values, copying or
moving union cells, allocation lifetime, cache locality, heap pressure,
serialized size, or garbage-collection cost attributable to a layout.

It also fixes four tags with widths `[2,0,1,2]`, two generated i32 candidates,
31 statically emitted steps, fixed NBT paths, a linear four-way dispatcher,
full active-payload consumption, and no payload mutation after construction.
It does not cover dynamic payload width, heterogeneous or nested payloads,
unknown tags, schema evolution, dynamic indexing, recursive matches,
branch-tree lowering, dead-payload elimination, profile-guided arm ordering,
score-only union representations, or compiler-generated dispatch
optimization. Data-pack loading, parsing, parsed-command memory, and runtime
storage memory are unmeasured.
