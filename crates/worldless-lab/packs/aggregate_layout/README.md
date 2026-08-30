# Aggregate layout data pack

This pack compares three command-storage layouts for a fixed-shape aggregate
of signed 32-bit integers without physical-world state.

## Contract

Each public entry point reads `worldless_lab:aggregate_layout/input`. The
input must contain exactly these fields:

```snbt
{length:<IntTag>,seed:<IntTag>,order:"record_major"|"field_major"}
```

`length` must be 1, 16, 64, or 128. `seed` may be any signed 32-bit integer.
Invalid input returns failure with value 0 before replacing internal work state
or updating public output. Validation round-trips `length` and `seed`
through scores into an IntTag scratch compound and uses an exact NBT partial
match, so other numeric tag types are not accepted through numeric coercion.

The public entry points are:

- `worldless_lab:aggregate_layout/record_compounds/run`;
- `worldless_lab:aggregate_layout/column_arrays/run`;
- `worldless_lab:aggregate_layout/flat_array/run`.

A successful invocation sets the `checksum` field in
`worldless_lab:aggregate_layout/output` and returns success with value 1. The
lab clears output storage before every invocation, so its checked output is
exactly `{checksum:<IntTag>}`.

## Layouts

Every logical record contains three IntTag fields named `x`, `y`, and `z`.
The variants allocate these equivalent layouts:

- `record_compounds`: `ListTag<CompoundTag>` in record order,
  `[{x:...,y:...,z:...},...]`;
- `column_arrays`: one compound containing
  `{x:[I;...],y:[I;...],z:[I;...]}`;
- `flat_array`: one interleaved `IntArrayTag`,
  `[I;x0,y0,z0,x1,y1,z1,...]`.

Each invocation allocates its complete zero-filled layout with one command.
It then fills all scalar slots in canonical record-major `x`, `y`, `z`
order from the same scoreboard generator:

```text
state = seed
value = state
state = wrapping_i32(state * 1664525 + 1013904223)
```

The input therefore does not privilege any target layout with an already
encoded aggregate.

## Workload

Every scalar is transformed independently seven times using signed 32-bit
wrapping arithmetic:

```text
value = wrapping_i32(value * 31 + 7)
```

`record_major` visits `x`, `y`, and `z` within each record before moving
to the next record. `field_major` visits every `x`, then every `y`, then
every `z`. The generated kernels use only fixed NBT paths and differ solely
in path order. They contain no function macros or runtime index arithmetic.

After the seventh round, every variant reads the final values in canonical
record-major order and folds them into a checksum starting at 1:

```text
checksum = wrapping_i32(checksum * 31 + value)
```

Initialization, scalar reads, arithmetic, writes, function calls, and checksum
folding have the same command count for every layout and traversal order.
Consequently, quota equality is a correctness condition of the experiment;
elapsed-time differences expose the selected NBT shape and access order.
Zero-allocation literal bytes and generated source bytes still differ and
should be reported separately from command quota.

This experiment targets fixed-length homogeneous integer aggregates. It does
not cover dynamic indexing, aggregate growth, heterogeneous fields, variable
lengths, schema evolution, or indirect field selection. Those concerns must
not be inferred from its results.

## Generated code size

The runtime pack payload (`pack.mcmeta` and `data`, excluding this README) is
2,472,219 bytes and contains 90 function resources with 26,656 nonblank command
lines. Most of that source is the statically unrolled fixed-path initialization,
update, and checksum code.

Each variant owns 29 function resources and 8,873 nonblank commands. The
variant-source column sums their file contents; the longest line is the
length-128 zero-allocation command. Byte measurements are ASCII bytes and the
line lengths exclude the newline.

| Layout | Variant source | Longest initialization line |
| --- | ---: | ---: |
| Record compounds | 826,063 | 1,880 |
| Column arrays | 824,223 | 874 |
| Flat array | 818,507 | 858 |

These sizes describe the generated pack. Pack loading and parsing are outside
the invocation timings below.

## Measurement

From the repository root, run correctness and persistent comparison with:

```sh
cargo run -p worldless-lab -- check --suite aggregate_layout --format text
cargo run --release -p worldless-lab -- compare --suite aggregate_layout \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Sample results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in the fixed table order.

| Order | Length | Record compounds `quota; median` | Column arrays `quota; median` | Flat array `quota; median` |
| --- | ---: | ---: | ---: | ---: |
| `record_major` | 1 | 218; 33.290 | 218; 33.000 | 218; 31.940 |
| `record_major` | 16 | 2,468; 409.041 | 2,468; 390.101 | 2,468; 370.491 |
| `record_major` | 64 | 9,668; 1,571.515 | 9,668; 1,527.905 | 9,668; 1,467.455 |
| `record_major` | 128 | 19,268; 3,175.560 | 19,268; 3,160.651 | 19,268; 2,975.750 |
| `field_major` | 1 | 218; 33.260 | 218; 33.140 | 218; 31.640 |
| `field_major` | 16 | 2,468; 394.971 | 2,468; 382.281 | 2,468; 364.411 |
| `field_major` | 64 | 9,668; 1,577.345 | 9,668; 1,515.125 | 9,668; 1,468.805 |
| `field_major` | 128 | 19,268; 3,171.501 | 19,268; 3,117.180 | 19,268; 2,967.660 |

Quota was exactly equal across all three layouts and both orders at each
length, as required by the experiment. At length 128, flat array had a 6.3
percent lower median than record compounds for `record_major` and a 6.4
percent lower median for `field_major` in this Worldless run. This supports
the flat representation for this fixed-width, fixed-path integer workload,
while retaining its interleaved indexing and generated-code tradeoffs.

Matched `record_major` and `field_major` medians differed by at most 3.4
percent. Because cases ran in a fixed order and these differences were small,
the sample does not establish that either traversal order is faster. The
results also do not extend to dynamic indexing, variable-size aggregates, or
Minecraft runtime performance.

### Minecraft Java Edition

The same eight inputs and all three public entry points were checked and
measured on Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31.
The host was the same Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The
server used Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no
players. The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` consecutive public calls. Calibration selected a power-of-two `R`, up
to 1,024, toward 100 ms while keeping `R` times the Worldless quota below
800,000. The maximum-`R` rule kept the length-1 batches below the preferred
50 ms floor. Each of two fresh JVMs discarded 20 warm-up batches and measured
15 per row; the second JVM traversed all rows in reverse order. The table pools
the 30 unfiltered batch averages. Each cell is
`R; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight required return value 1. Both preflight and
post-batch checks required exact public output without extra fields and the
expected internal checksum. They also checked a compound-list layout of length
`N` for record compounds, exactly the `x`, `y`, and `z` IntArrays of length `N`
for column arrays, and an IntArray of length `3N` for flat array. Input
installation, calibration, preflight, and all result and layout checks were
outside the stopwatch.

Each timed public call still includes input validation, zero allocation, the
canonical generator fill, all seven update rounds, checksum folding, and
output publication. The array layouts use fixed-index stores into
their preallocated IntArrays; this workload does not append elements or copy
an array for each update.

The two JVMs recorded 172 and 183 young collections, no full collections, and
maximum pauses of 25.539 and 50.432 ms. Samples were not filtered, so pauses
inside timed batches remain in the results.

| Order | Length | Record compounds `R; median / p95` | Column arrays `R; median / p95` | Flat array `R; median / p95` |
| --- | ---: | ---: | ---: | ---: |
| `record_major` | 1 | 1,024; 25.391 / 26.367 | 1,024; 24.414 / 25.391 | 1,024; 23.438 / 24.414 |
| `record_major` | 16 | 256; 287.109 / 296.875 | 256; 277.344 / 281.250 | 256; 265.625 / 269.531 |
| `record_major` | 64 | 64; 1,148.438 / 1,218.750 | 64; 1,125.000 / 1,156.250 | 64; 1,078.125 / 1,140.625 |
| `record_major` | 128 | 32; 2,343.750 / 2,406.250 | 32; 2,281.250 / 2,375.000 | 32; 2,234.375 / 2,281.250 |
| `field_major` | 1 | 1,024; 24.902 / 25.391 | 1,024; 24.414 / 25.391 | 1,024; 23.438 / 24.414 |
| `field_major` | 16 | 256; 283.203 / 296.875 | 256; 277.344 / 285.156 | 256; 265.625 / 273.438 |
| `field_major` | 64 | 64; 1,164.062 / 1,203.125 | 64; 1,125.000 / 1,140.625 | 64; 1,093.750 / 1,109.375 |
| `field_major` | 128 | 32; 2,359.375 / 2,437.500 | 32; 2,281.250 / 2,375.000 | 32; 2,156.250 / 2,281.250 |

At length 128, flat array had a 4.7 percent lower median than record compounds
for `record_major` and an 8.6 percent lower median for `field_major`. Traversal
order did not have a consistent ranking as length changed, so these samples do
not establish a generally faster order. They support flat fixed-index storage
for this bounded end-to-end workload, not for dynamic indexing, resizing, or
append-heavy aggregate operations.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock, while Worldless measures individual invocations using its
host clock. Runtime implementation, batching, and row-order controls also
differ, so comparisons should remain within each runtime.

## Failure boundaries

Input validation was checked separately with missing and extra fields, wrong
NBT types, unsupported lengths, invalid order strings, and signed-32-bit seed
boundaries. Invalid calls returned 0 without changing pre-existing work or
output state; the minimum and maximum valid IntTag seeds both succeeded.

Command-limit interruption is not a validation failure and does not roll back
mutations. A length-128 row used Worldless quota 19,268. A direct invocation
with that exact limit reported `CommandLimitExceeded` even though its expected
checksum had already been published; 19,269 succeeded. At limit 100, the work
layout was only partially initialized and the old public output was unchanged.
A subsequent full-limit call on the same VM succeeded because `prepare`
replaced the interrupted work state. Execution status, rather than apparently
complete output, is authoritative at a quota boundary.

A separate fresh Minecraft JVM ran the length-128 `record_major`
record-compounds row with an empirical command-sequence limit of 100. The
server reported stopping after exactly 100 commands; the surrounding
`execute store` retained its return sentinel of -999, the expected public
output was absent, and the internal checksum existed at 0. The preallocated
layout was still a compound list of length 128. Restoring the normal limit of
1,000,000 and rerunning the complete preflight on the same JVM succeeded. This
probe was not mixed into calibration, warm-up, timing, or GC results above.
