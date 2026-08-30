# Integer sorting data pack

This data pack compares two signed 32-bit integer sorting algorithms using
only data-pack functions, scoreboards, command storage, and function macros.
It operates entirely on VM-owned logical data and does not observe or modify a
physical Minecraft world.

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
Unlike insertion sort, every input order requires all merge passes, but its
growth is bounded by `O(n log n)`.

| Variant | Best time | Average time | Worst time | Working NBT |
| --- | --- | --- | --- | --- |
| Insertion | `O(n)` | `O(n²)` | `O(n²)` | One saved key |
| Bottom-up merge | `O(n log n)` | `O(n log n)` | `O(n log n)` | A second array of up to `n` values |

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

## Benchmark

Reproduce the comparison with a release build:

```sh
cargo run --release -p worldless-lab -- compare \
  --suite int_sort --samples 31 --format text
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
