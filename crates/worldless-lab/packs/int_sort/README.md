# Integer sorting data pack

This data pack compares two signed 32-bit integer sorting algorithms using
only data-pack functions, scoreboards, command storage, and function macros.
All state lives in command storage and scoreboards; the pack does not observe
or modify blocks, entities, or other physical world state.

## Lab contract

Each lab entry point reads this exact compound from command storage
`worldless_lab:int_sort/input`:

```snbt
{values:[I;3,-1,2]}
```

The only accepted field is `values`, and its value must be an `IntArrayTag`.
An invalid compound returns failure before replacing the sorting work state or
output. A successful invocation writes the ascending signed-integer result to
`worldless_lab:int_sort/output`:

```snbt
{values:[I;-1,2,3]}
```

The entry points are:

- `worldless_lab:int_sort/insertion/run`;
- `worldless_lab:int_sort/bottom_up_merge/run`.

Both return success with value 1 after writing the output. The `int_sort:state`
and `int_sort:validation` storages and the `int_sort` scoreboard objective are
internal.

## Algorithms

### Insertion sort

The insertion variant grows a sorted prefix from left to right. For each new
key, it scans the prefix backwards until it reaches a value less than or equal
to the key. If the key must move, the pack removes it from the int array and
inserts it once at the discovered destination through a function macro.

The backwards scan is adaptive: an already sorted array needs one comparison
per new key, while a reverse-sorted or typical unordered array needs a
quadratic number of comparisons. Removing and reinserting the key updates the
intervening array indices as part of those NBT operations. Each remains one
data-pack command for quota accounting, while its internal array work is still
reflected in elapsed time.

### Bottom-up merge sort

The merge variant starts with runs of width 1 and repeatedly merges adjacent
runs into a fresh `IntArrayTag`. After each pass it replaces the source array
with the merged array and doubles the run width. Run bounds are clamped to the
input length, so odd-sized tails require no padding or sentinel value.

Each merge reads the current left and right values through dynamic-index
macros and appends the smaller value. Equal values are taken from the left run.
Unlike insertion sort, every input order requires all merge passes. Its
comparison and data-pack command growth is bounded by `O(n log n)`.

| Variant | Best command growth | Average command growth | Worst command growth | Working NBT |
| --- | --- | --- | --- | --- |
| Insertion | `O(n)` | `O(n²)` | `O(n²)` | One saved key |
| Bottom-up merge | `O(n log n)` | `O(n log n)` | `O(n log n)` | A second array of up to `n` values |

Those bounds count the algorithm's comparisons and data-pack operations, not
the native work hidden inside one NBT command. Worldless stores primitive
arrays in mutable Rust vectors: appending is amortized `O(1)`, while an indexed
remove or insert shifts the affected suffix. In Minecraft
`26.3-snapshot-10`, the three primitive array tags instead hold fixed Java
arrays; every append, insert, or remove allocates a replacement and copies the
preserved elements. For this implementation, insertion keeps its stated
best/average/worst bounds, but merge's repeated array construction adds
`Theta(n²)` copying per pass and `Theta(n² log n)` across all passes. This is a
runtime-representation cost, not an observable data-pack compatibility rule.

Values are compared directly as scoreboard scores. Neither algorithm subtracts
one input value from another, so `-2147483648` and `2147483647` do not create a
comparison overflow.

## Correctness cases

The lab suite derives each expected result by sorting the source values in
Rust. The registered inputs, command limit, and expected-output derivation live
in [the suite source](../../src/suites/int_sort.rs). Its eleven checked cases
cover:

- empty and single-element arrays;
- duplicate, negative, minimum, and maximum signed integers;
- sorted, reverse-sorted, and deterministically permuted arrays;
- lengths 8, 32, and 128; and
- an odd length whose final merge run is incomplete.

Run the complete correctness check from the repository root:

```sh
cargo run -p worldless-lab -- check --suite int_sort --format text
```

## Benchmarks

### Worldless VM

Reproduce the comparison with a release build:

```sh
cargo run --release -p worldless-lab -- compare \
  --suite int_sort --execution fresh --samples 31 --format text
```

The table below was measured on 2026-08-30 on an AMD Ryzen 9 9950X3D running
Linux 7.0.0-29-generic with rustc 1.98.0. Every sample used a fresh VM and cold
macro cache. The comparison first ran correctness checks and one untimed quota
invocation per row, but those invocations did not share VM state or a macro
cache with the timed samples. Pack compilation, VM construction, input
installation, and output verification were outside the timer. The timed region
was the complete public entry-point invocation, including input validation,
sorting, and output storage.

Times are medians of 31 unfiltered samples. They are environment-dependent.
Command quota is deterministic for a given case and implementation; the suite
limit was 131,072 commands.

| Case | Length | Insertion quota | Insertion median (µs) | Merge quota | Merge median (µs) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `empty` | 0 | 28 | 5.150 | 29 | 5.350 |
| `singleton` | 1 | 28 | 4.840 | 29 | 5.050 |
| `mixed_extremes_7` | 7 | 253 | 185.020 | 503 | 257.211 |
| `sorted_8` | 8 | 196 | 162.220 | 517 | 257.171 |
| `reverse_8` | 8 | 427 | 239.401 | 529 | 257.061 |
| `sorted_32` | 32 | 772 | 484.131 | 2,973 | 1,565.854 |
| `reverse_32` | 32 | 5,143 | 2,630.707 | 3,053 | 1,547.984 |
| `permuted_32` | 32 | 2,680 | 1,376.514 | 3,490 | 1,930.555 |
| `sorted_128` | 128 | 3,076 | 1,733.405 | 15,737 | 8,166.443 |
| `reverse_128` | 128 | 75,847 | 36,566.832 | 16,185 | 8,230.563 |
| `permuted_128` | 128 | 38,968 | 19,157.843 | 19,244 | 10,669.690 |

The fixed merge passes do not pay off for small or already sorted inputs. At
128 sorted values, the merge median was 4.71 times the insertion median. Merge
sort overtook insertion on the 32-value reverse case. At 128 values, the
insertion median was 4.44 times the merge median for the reverse case and 1.80
times the merge median for the permuted case. The exact crossover therefore
depends on both input length and existing order; the sampled cases do not
define a universal threshold.

The largest measured command use was 75,847 for reverse insertion at length
128. Larger or more adversarial inputs must be measured against the caller's
command limit rather than inferred from the 128-element results.

### Minecraft Java Edition

The same eleven inputs and both public entry points were also checked and
measured on the actual Minecraft Java dedicated server. This snapshot was
taken on 2026-08-30 on the same AMD Ryzen 9 9950X3D host, using Minecraft
`26.3-snapshot-10`, Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`,
`--nogui`, and no players. The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`.
The command-sequence game rule was 1,000,000.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` consecutive calls to one public entry point. Calibration selected a
power-of-two `R` toward 100 ms while keeping `R` times the Worldless quota
below 800,000. For every row, each of two fresh server JVMs discarded 20
warm-up batches and measured 15; the second JVM traversed all rows in reverse
order. The table pools the resulting 30 unfiltered batch averages. Median and
nearest-rank p95 are shown in microseconds per call, obtained by dividing each
batch duration by `R`; fractional values do not imply sub-millisecond stopwatch
resolution. These are sustained warm-call measurements, not individual-call
latency samples.

Input installation, the single-call correctness preflight, and output
verification were outside each timed batch; calibration batches were excluded
from the reported samples. The timed region contained the complete public
calls, including validation, state preparation, sorting, and output writes.
Every preflight and every batch produced the exact expected output. Across the
complete JVM lifetimes of the combined sort-and-map run, the GC logs recorded
612 and 605 young collections and no full collection; the maximum pauses were
47.780 and 48.367 ms. Samples were not filtered, so any pause inside a timed
batch remains in the result.

| Case | Length | Insertion `R` | Insertion median (µs) | Insertion p95 (µs) | Merge `R` | Merge median (µs) | Merge p95 (µs) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `empty` | 0 | 16,384 | 4.425 | 4.578 | 16,384 | 4.547 | 4.700 |
| `singleton` | 1 | 16,384 | 3.784 | 3.967 | 16,384 | 3.845 | 4.089 |
| `mixed_extremes_7` | 7 | 1,024 | 33.203 | 34.180 | 512 | 95.703 | 97.656 |
| `sorted_8` | 8 | 2,048 | 24.902 | 25.391 | 1,024 | 95.703 | 98.633 |
| `reverse_8` | 8 | 1,024 | 53.711 | 55.664 | 1,024 | 95.703 | 97.656 |
| `sorted_32` | 32 | 256 | 226.562 | 230.469 | 64 | 812.500 | 843.750 |
| `reverse_32` | 32 | 64 | 1,437.500 | 1,468.750 | 128 | 816.406 | 828.125 |
| `permuted_32` | 32 | 128 | 734.375 | 742.188 | 64 | 1,000.000 | 1,015.625 |
| `sorted_128` | 128 | 64 | 890.625 | 921.875 | 16 | 4,437.500 | 4,500.000 |
| `reverse_128` | 128 | 4 | 20,750.000 | 21,500.000 | 16 | 4,437.500 | 4,500.000 |
| `permuted_128` | 128 | 8 | 10,750.000 | 10,875.000 | 16 | 5,687.500 | 5,812.500 |

The measured crossover remained input-sensitive despite Minecraft's more
expensive primitive-array appends. At 128 sorted values, merge was 4.98 times
slower than insertion. On the reverse and permuted 128-value cases, insertion
was respectively 4.68 and 1.89 times slower than merge. At length 32,
insertion still won the deterministic permutation while merge won the reverse
case. These rankings describe the tested warm server and sizes, not a general
crossover or the asymptotic behavior at larger `n`.

The Worldless and Minecraft absolute times above are not paired runtime
latencies: Worldless starts every timed sample in a fresh VM with a cold macro
cache, whereas Minecraft batches repeated calls in a warmed persistent JVM.
The tables are useful for comparing variants within each runtime and for
exposing runtime-specific NBT costs, but an absolute Worldless/Minecraft speed
ratio would require a matched cache-state and lifecycle protocol.
