# Indirect array access data pack

This pack compares three ways to read a runtime-selected element from a
16-element integer array without physical-world state.

## Contract

Each public entry point reads
`worldless_lab:indirect_access/input`. The input must contain exactly two
fields:

```snbt
{values:[I;...16 integers...],indices:[I;...63 integers...]}
```
Both fields must be `IntArrayTag` values, and every index must be in
`0..15`. Invalid input fails before the pack replaces its work state or
updates the public output.

A successful invocation reads the values in index order and computes a signed
32-bit wrapping checksum starting at 1:

```text
checksum = checksum * 31 + value
```

It writes `checksum` to `worldless_lab:indirect_access/output` and returns
success with value 1.

The public entry points are:

- `worldless_lab:indirect_access/dynamic_path/run`;
- `worldless_lab:indirect_access/specialized_call/run`;
- `worldless_lab:indirect_access/binary_dispatch/run`.

`dynamic_path` expands the selected NBT array path in a function macro.
`specialized_call` expands a function identifier and delegates to one of 16
fixed-path leaf functions. `binary_dispatch` uses a four-level static score
tree to reach those same leaves; every node contains two range checks.

The suite uses 63 accesses so repeated persistent invocations preserve the
intended eight-versus-nine macro-cache working-set boundary. Cache behavior is
a performance observation, not part of the data-pack correctness contract.
The registered workloads cover one repeated index, sequential working sets of
8, 9, and 16 indices, and a nine-index workload with one hot index.

Persistent comparison creates one VM per case-and-variant row, discards the
requested warm-up invocations, and then measures repeated invocations on that
same VM. Rows never share state. Each timed invocation includes validation,
all 63 reads, checksum folding, and output publication; pack loading, VM
creation, input installation, output clearing, and result verification remain
outside the timer. Use `--execution fresh` without `--warmup` for the cold
baseline.

From the repository root, run correctness and persistent comparison with:

```sh
cargo run -p worldless-lab -- check --suite indirect_access --format text
cargo run --release -p worldless-lab -- compare --suite indirect_access \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Sample results

### Worldless

The persistent command above produced these median times on 2026-08-30 on an
AMD Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Times are
environment-dependent.

| Workload | Dynamic path (µs) | Specialized call (µs) | Binary dispatch (µs) |
| --- | ---: | ---: | ---: |
| `repeat_1` | 165.9 | 176.4 | 240.4 |
| `cycle_8` | 169.5 | 179.1 | 248.0 |
| `cycle_9` | 508.9 | 327.6 | 246.0 |
| `cycle_16` | 515.6 | 329.8 | 246.8 |
| `hot_9` | 352.7 | 272.3 | 209.7 |

Command use was independent of the workload: 865 for dynamic path, 928 for
specialized call, and 1,684 for binary dispatch. Macro-cache misses therefore
appear in elapsed time, not command quota. On this Worldless run, the macro
variants won when their complete working set fit the cache, while binary
dispatch won for the wider or mixed sets despite using more commands.

### Minecraft Java Edition

The same five inputs and all three public entry points were also checked and
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
fractional values result from dividing an integer-millisecond batch duration
by `R`. These are sustained warm-call measurements, not individual-call
latency samples.

Input installation, the single-call correctness preflight, and output
verification were outside each timed batch; calibration batches were excluded
from the reported samples. The timed region contained the complete public
calls, including validation, all 63 reads, checksum folding, and output
publication. Every preflight returned 1 and produced exactly the expected
output, and every measured batch ended with the expected checksum. The two GC
logs recorded 113 and 111 young collections and no full collection; maximum
pauses were 48.629 and 24.483 ms. Samples were not filtered, so a pause inside
a timed batch remains in the result.

| Workload | Dynamic path `R; median / p95` (µs) | Specialized call `R; median / p95` (µs) | Binary dispatch `R; median / p95` (µs) |
| --- | ---: | ---: | ---: |
| `repeat_1` | 512; 93.750 / 107.422 | 512; 95.703 / 111.328 | 256; 136.719 / 144.531 |
| `cycle_8` | 512; 93.750 / 99.609 | 512; 98.633 / 101.562 | 256; 144.531 / 152.344 |
| `cycle_9` | 256; 191.406 / 199.219 | 512; 136.719 / 140.625 | 256; 144.531 / 148.438 |
| `cycle_16` | 512; 187.500 / 193.359 | 512; 130.859 / 134.766 | 256; 140.625 / 144.531 |
| `hot_9` | 512; 132.812 / 134.766 | 512; 113.281 / 130.859 | 256; 144.531 / 179.688 |

The dynamic-path median rose from 93.750 µs for `cycle_8` to 191.406 µs for
`cycle_9`, consistent with the intended eight-entry macro-cache boundary. The
specialized-call increase at the same boundary was smaller, from 98.633 to
136.719 µs. Dynamic path was fastest for `repeat_1` and `cycle_8`, while
specialized call was fastest for `cycle_9`, `cycle_16`, and `hot_9`. Binary
dispatch stayed comparatively workload-independent at 136.719 to 144.531 µs
median, but its higher command use did not buy the best median on this server.

The wider-workload ranking differs from the Worldless result, where binary
dispatch won. A compiler cost model should therefore treat execution runtime
as an input instead of assigning one universal lowering. Both measurements use
persistent warm macro caches, but their absolute times are not paired
latencies: Minecraft times batches whose calls after the first begin with the
previous public output installed, whereas Worldless clears that output before
each individually timed call. Stopwatch resolution and runtime implementation
also differ, so comparisons should remain within each runtime.
