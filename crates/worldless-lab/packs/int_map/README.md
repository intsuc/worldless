# Integer map representations data pack

This data pack compares three representations for a batch of signed 32-bit
integer map lookups. It uses only data-pack functions, scoreboards, command
storage, and function macros. All state lives in command storage and
scoreboards; the pack does not observe or modify blocks, entities, or other
physical world state.

## Lab contract

Each public entry point reads the following three fields from command storage
`worldless_lab:int_map/input`:

```snbt
{keys:[I;7,7,-1],values:[I;3,5,9],queries:[I;7,0,-1]}
```

They are the only accepted top-level fields, all three must be `IntArrayTag`
values, and `keys` and `values` must have equal lengths. The pack registers
`keys[i] -> values[i]` in input order, so the last value wins when a key is
repeated. It rebuilds this logical map for every invocation; it is not a
persistent mutation API.

Success writes the `found` and `values` fields in
`worldless_lab:int_map/output`, preserving query order:

```snbt
{found:[B;1b,0b,1b],values:[I;5,0,9]}
```

`found[i]` is 1 for a hit and 0 for a miss. `values[i]` contains the mapped
value for a hit and the placeholder 0 for a miss, so callers must use `found`
to distinguish a missing key from a stored value of zero.

The entry points are:

- `worldless_lab:int_map/linear_scan/run`;
- `worldless_lab:int_map/nbt_compound/run`;
- `worldless_lab:int_map/scoreboard/run`.

Each returns success with value 1 after writing the result fields. Missing,
extra, wrongly typed, or unequal-length input fails before replacing the map
work state or public result fields. The `int_map:state` and
`int_map:validation` storages and the `int_map` and `int_map_values` scoreboard
objectives are internal.

## Representations and algorithms

Let `n` be the number of input pairs, `q` the number of queries, and `u` the
number of distinct keys. The command-growth table describes the
representation-specific work; shared validation, input copying, and result
emission are not included.

| Variant | Derived-index build commands | Lookup commands | Additional map state |
| --- | --- | --- | --- |
| Linear scan | None | Worst-case `Theta(nq)` | No derived index |
| NBT compound | `Theta(n)` | `Theta(q)` | Up to `u` compound fields |
| Scoreboard | `Theta(n)` | `Theta(q)` | Up to `u` fake-holder scores |

### Linear scan

This variant queries the copied parallel arrays directly. It starts at the
last key and scans backwards, returning the first match. The direction makes
duplicate keys obey the last-value-wins contract without building another
index. A query for the last pair needs one comparison. A miss, or a query for
a uniquely occurring key in the first pair, scans all `n` entries.

### NBT compound

This variant first materializes a compound whose field names are the quoted
decimal forms of the integer keys. Assigning pairs in input order overwrites a
field when a key repeats. Each query performs a dynamic-path existence check
and reads the value only on a hit.

### Scoreboard

This variant materializes each distinct key as a named fake score holder in a
dedicated objective. Input-order assignment gives the same overwrite
semantics, and a query captures both the success and result of a scoreboard
lookup to distinguish a missing key from value zero. These named holders are
scoreboard entries, not Minecraft players or entities.

The NBT and scoreboard variants use a constant number of data-pack commands
per lookup, but this is not a claim of constant elapsed time. Native storage
work and dynamic macro compilation are included in the timing measurements.

The actual target server has another shared cost that command counts do not
show. In Minecraft `26.3-snapshot-10`, appending to a `ByteArrayTag` or
`IntArrayTag` allocates a replacement Java primitive array and copies the old
contents. All three variants append every query result to both output arrays,
so result emission alone performs `Theta(q²)` element copying and temporary
allocation. Worldless's growable vectors make the corresponding append
sequence amortized `Theta(q)`. With average constant-time native
compound/scoreboard access as a model, the Minecraft storage work is roughly
`Theta(n + nq + q²)` for linear scan and `Theta(n + q²)` for either derived
index. These are properties of the measured Minecraft implementation, not
observable data-pack compatibility requirements.

## Correctness cases

The lab suite derives expected results with a Rust integer map. The registered
inputs, expected-output derivation, and command limit live in
[the suite source](../../src/suites/int_map.rs). Its twelve checked cases cover:

- empty maps and queries against an empty map;
- singleton hits and misses, stored zero, and missing keys;
- repeated keys with last-value-wins behavior;
- minimum and maximum signed 32-bit keys and values;
- complete hit and miss workloads at 32 entries;
- 128 entries with no queries and with every key queried; and
- repeated last-entry, repeated first-entry, and mixed hit/miss workloads.

Run all 12 cases against all three representations from the repository root:

```sh
cargo run -p worldless-lab -- check --suite int_map --format text
```

## Benchmarks

### Worldless VM

Reproduce the comparison with a release build:

```sh
cargo run --release -p worldless-lab -- compare \
  --suite int_map --samples 31 --format text
```

The tables below were measured on 2026-08-30 on an AMD Ryzen 9 9950X3D
running Linux 7.0.0-29-generic with rustc 1.98.0. Every timed sample started in
a fresh VM with an empty macro cache; repeated instantiations could still be
reused within that invocation. The comparison first ran correctness checks and
one untimed quota invocation per row, but those invocations shared neither VM
state nor macro caches with the timed samples.

Pack compilation, VM construction, input installation, and output verification
were outside the timer. The timed region was the complete public entry point,
including validation, input copying, representation construction, all queries,
result emission, and output storage. This is therefore an end-to-end batch
comparison, not a steady-state lookup benchmark. In particular, linear scan
queries the copied input arrays without materializing an index, while the
other two variants build an index on every invocation.

Times are medians of 31 unfiltered samples and are environment-dependent.
Command quota is deterministic for a given case and implementation; the suite
limit was 262,144 commands.

#### Command quota

| Case | Input pairs | Queries | Linear | NBT compound | Scoreboard |
| --- | ---: | ---: | ---: | ---: | ---: |
| `empty` | 0 | 0 | 56 | 59 | 60 |
| `empty_map_misses_4` | 0 | 4 | 156 | 163 | 160 |
| `singleton_hit_miss` | 1 | 3 | 162 | 149 | 149 |
| `zero_and_missing` | 1 | 2 | 126 | 122 | 124 |
| `duplicate_extremes` | 5 | 4 | 261 | 216 | 230 |
| `hits_32` | 32 | 32 | 5,672 | 1,243 | 1,308 |
| `misses_32` | 32 | 32 | 10,072 | 1,211 | 1,308 |
| `entries_128_no_queries` | 128 | 0 | 56 | 1,339 | 1,852 |
| `hits_128` | 128 | 128 | 77,816 | 4,795 | 5,052 |
| `hot_last_128` | 128 | 128 | 4,664 | 4,795 | 5,052 |
| `hot_first_128` | 128 | 128 | 150,968 | 4,795 | 5,052 |
| `mixed_128` | 128 | 128 | 114,552 | 4,731 | 5,052 |

#### Median elapsed time

| Case | Input pairs | Queries | Linear (µs) | NBT compound (µs) | Scoreboard (µs) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `empty` | 0 | 0 | 81.790 | 83.210 | 83.940 |
| `empty_map_misses_4` | 0 | 4 | 116.550 | 155.061 | 136.300 |
| `singleton_hit_miss` | 1 | 3 | 118.841 | 140.300 | 129.410 |
| `zero_and_missing` | 1 | 2 | 108.160 | 131.590 | 121.070 |
| `duplicate_extremes` | 5 | 4 | 159.851 | 213.461 | 192.711 |
| `hits_32` | 32 | 32 | 2,727.237 | 1,017.703 | 864.253 |
| `misses_32` | 32 | 32 | 4,985.413 | 1,009.583 | 871.252 |
| `entries_128_no_queries` | 128 | 0 | 79.980 | 1,595.794 | 1,557.004 |
| `hits_128` | 128 | 128 | 38,307.160 | 3,674.800 | 3,126.098 |
| `hot_last_128` | 128 | 128 | 1,281.483 | 2,606.717 | 2,546.876 |
| `hot_first_128` | 128 | 128 | 75,132.936 | 2,608.737 | 2,558.356 |
| `mixed_128` | 128 | 128 | 56,770.819 | 3,677.669 | 3,142.829 |

Materializing an unused index is expensive: with 128 pairs and no queries,
the NBT and scoreboard medians were about 20 times the linear median. Linear
scan also won the repeated-last-key case because every query matched after one
comparison; the NBT and scoreboard medians were respectively 2.03 and 1.99
times the linear median.

The result reversed when linear scan traversed much of the array. On all 128
keys, the linear median was 10.42 times the NBT compound median and 12.25 times
the scoreboard median. On the repeated-first-key case, the corresponding
ratios were 28.80 and 29.37. For the all-key and mixed 128-entry cases, the
scoreboard medians were roughly 15 percent lower than the NBT compound medians
despite slightly higher data-pack command counts.

The largest measured quota use was 150,968 for repeated first-key linear scan.
Input order, query distribution, repeated macro instantiations, and native VM
costs all affect the crossover, so these cases do not establish a universal
threshold. These measurements describe Worldless on this machine; larger
workloads must be measured against their caller's own command limit.

### Minecraft Java Edition

The same twelve inputs and all three public entry points were also checked and
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
order. The table pools the resulting 30 unfiltered batch averages. Each result
cell is `R; median / nearest-rank p95`, with times in microseconds per call;
fractional values do not imply sub-millisecond stopwatch resolution. These are
sustained warm-call measurements, not individual-call latency samples.

Input installation, the single-call correctness preflight, and output
verification were outside each timed batch; calibration batches were excluded
from the reported samples. The timed region contained the complete public
calls, including validation, input copying, optional index construction, all
queries, result emission, and public output writes. Every
preflight and every batch produced the exact expected output. Across the
complete JVM lifetimes of the combined sort-and-map run, the GC logs recorded
612 and 605 young collections and no full collection; the maximum pauses were
47.780 and 48.367 ms. Samples were not filtered, so any pause inside a timed
batch remains in the result.

| Case | Input pairs | Queries | Linear `R; median / p95` (µs) | NBT compound `R; median / p95` (µs) | Scoreboard `R; median / p95` (µs) |
| --- | ---: | ---: | ---: | ---: | ---: |
| `empty` | 0 | 0 | 8,192; 8.728 / 8.911 | 8,192; 9.155 / 9.399 | 8,192; 9.583 / 9.766 |
| `empty_map_misses_4` | 0 | 4 | 4,096; 21.851 / 22.461 | 4,096; 23.438 / 23.926 | 4,096; 25.269 / 25.879 |
| `singleton_hit_miss` | 1 | 3 | 4,096; 21.973 / 22.705 | 4,096; 20.874 / 21.484 | 4,096; 21.362 / 22.217 |
| `zero_and_missing` | 1 | 2 | 4,096; 17.090 / 17.822 | 4,096; 17.456 / 18.066 | 4,096; 17.944 / 18.799 |
| `duplicate_extremes` | 5 | 4 | 2,048; 34.912 / 36.133 | 2,048; 31.982 / 33.203 | 2,048; 32.471 / 34.668 |
| `hits_32` | 32 | 32 | 64; 1,507.812 / 1,546.875 | 128; 468.750 / 484.375 | 128; 390.625 / 414.062 |
| `misses_32` | 32 | 32 | 32; 2,750.000 / 2,843.750 | 128; 464.844 / 476.562 | 128; 414.062 / 429.688 |
| `entries_128_no_queries` | 128 | 0 | 8,192; 8.301 / 8.423 | 64; 773.438 / 796.875 | 128; 718.750 / 734.375 |
| `hits_128` | 128 | 128 | 4; 21,250.000 / 21,750.000 | 32; 1,812.500 / 1,875.000 | 128; 1,535.156 / 1,578.125 |
| `hot_last_128` | 128 | 128 | 128; 812.500 / 843.750 | 64; 1,429.688 / 1,468.750 | 64; 1,351.562 / 1,375.000 |
| `hot_first_128` | 128 | 128 | 2; 41,250.000 / 42,500.000 | 64; 1,421.875 / 1,453.125 | 64; 1,328.125 / 1,375.000 |
| `mixed_128` | 128 | 128 | 2; 31,500.000 / 32,500.000 | 32; 1,843.750 / 1,937.500 | 32; 1,593.750 / 1,687.500 |

The server results preserve the central position-sensitive behavior. On all
128 keys, linear scan was 13.84 times the scoreboard median; on repeated
first-key and mixed workloads, it was 31.06 and 19.76 times the scoreboard
median. Repeating the last input key instead made linear scan the fastest: NBT
compound and scoreboard were 1.76 and 1.66 times its median. The first-key
linear workload was 50.77 times slower than the otherwise identical last-key
workload.

With 128 entries and no queries, the complete NBT compound and scoreboard calls
took 773.438 and 718.750 µs, while linear scan, which materializes no index,
took 8.301 µs. On the all-hit and mixed 128-query workloads, scoreboard was
15.30% and 13.56% below NBT compound. The shared quadratic output-array copying
is included in all three numbers; the table does not isolate it from command,
macro, lookup, or index-build work.

The Worldless and Minecraft absolute times above are not paired runtime
latencies: Worldless starts every timed sample in a fresh VM with a cold macro
cache, whereas Minecraft batches repeated calls in a warmed persistent JVM.
The tables are useful for comparing representations within each runtime and
for exposing runtime-specific NBT costs, but an absolute Worldless/Minecraft
speed ratio would require a matched cache-state and lifecycle protocol.
