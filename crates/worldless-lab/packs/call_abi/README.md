# Call ABI data pack

This pack compares four calling conventions for a sequential integer leaf
function without physical-world state.

## Contract

Each public entry point reads `worldless_lab:call_abi/input`. The input must
contain exactly two fields:

```snbt
{a:[I;...],b:[I;...]}
```

Both fields must be `IntArrayTag` values with the same length, and that length
must be either 1 or 63. Invalid input fails before the pack replaces its work
state or updates the public output.

For every argument pair, the leaf computes signed 32-bit wrapping arithmetic:

```text
leaf = ((a * 31 + b) * 31 + 7)
checksum = checksum * 31 + leaf
```

The checksum starts at 1. A successful invocation writes `checksum` to
`worldless_lab:call_abi/output` and returns success with value 1.

The public entry points are:

- `worldless_lab:call_abi/score_slot/run`;
- `worldless_lab:call_abi/score_return/run`;
- `worldless_lab:call_abi/storage_return/run`;
- `worldless_lab:call_abi/macro_return/run`.

`score_slot` transfers arguments through fixed scoreboard slots and leaves
the result in another shared score slot. `score_return` uses the same score
arguments but returns the result through the function command value.
`storage_return` marshals the caller's score values into a fixed compound
frame, reloads them in the callee, and uses command return. `macro_return`
passes that same compound as function-macro arguments, specializes the
argument-loading commands, and also uses command return.

All four conventions deliberately use singleton argument, local, and result
slots. The experiment measures sequential leaf calls; nested calls,
reentrancy, caller-save spilling, and dynamic frame stacks are outside its
contract.

## Workloads and measurement

The argument pair for integer `k` is
`(i32::MIN + k, i32::MAX - 17 * k)`. The registered workloads contain one
call, 63 repetitions of pair zero, cycles over 8 or 9 distinct pairs, and 63
unique pairs. This separates fixed invocation cost from sustained cost and
places the macro convention on both sides of the eight-entry cache working-set
boundary. Cache behavior is a performance observation, not part of the
correctness contract.

Persistent comparison creates one VM per case-and-variant row, discards the
requested warm-up invocations, and then measures repeated invocations on that
same VM. Rows never share state. Each timed invocation includes validation,
argument loading and transfer, leaf calls, checksum folding, and output
publication; pack loading, VM creation, input installation, output clearing,
and result verification remain outside the timer. Use `--execution fresh`
without `--warmup` for the cold baseline.

From the repository root, run correctness and persistent comparison with:

```sh
cargo run -p worldless-lab -- check --suite call_abi --format text
cargo run --release -p worldless-lab -- compare --suite call_abi \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Sample results

### Worldless

The persistent command above produced these median times on 2026-08-31 on an
AMD Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Times are
environment-dependent.

| Workload | Score slot (µs) | Score return (µs) | Storage return (µs) | Macro return (µs) |
| --- | ---: | ---: | ---: | ---: |
| `single` | 10.430 | 10.770 | 11.030 | 10.880 |
| `repeat_1` | 178.390 | 193.600 | 209.001 | 196.270 |
| `cycle_8` | 178.601 | 191.420 | 214.420 | 196.671 |
| `cycle_9` | 179.391 | 190.711 | 210.701 | 451.182 |
| `unique_63` | 179.351 | 193.910 | 209.841 | 452.261 |

For the 63-call workloads, command use was independent of argument values:
995 for score slot, 1,121 for score return, 1,436 for storage return, and 1,247
for macro return. The corresponding single-call uses were 65, 67, 72, and 69.
Macro instantiation and cache misses therefore appear in elapsed time, not
command quota.

Score slot was fastest throughout this Worldless run. With a working set that
fit the macro cache, macro return remained close to score return and below
storage return. Its median rose from 196.671 µs for `cycle_8` to 451.182 µs
for `cycle_9`, while the three non-macro conventions remained nearly
workload-independent. The fixed-frame storage convention paid the highest
command cost but avoided specialization churn.

### Minecraft Java Edition

The same five inputs and all four public entry points were also checked and
measured on the actual Minecraft Java dedicated server. This snapshot was
taken on 2026-08-31 on the same AMD Ryzen 9 9950X3D host, using Minecraft
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
calls, including validation, argument transfer, leaf execution, checksum
folding, and output publication. Every preflight returned 1 and produced
exactly the expected output, and every measured batch ended with the expected
checksum. The two GC logs recorded 156 and 159 young collections and no full
collection; maximum pauses were 24.950 and 48.420 ms. Samples were not
filtered, so a pause inside a timed batch remains in the result.

| Workload | Score slot `R; median / p95` (µs) | Score return `R; median / p95` (µs) | Storage return `R; median / p95` (µs) | Macro return `R; median / p95` (µs) |
| --- | ---: | ---: | ---: | ---: |
| `single` | 8,192; 6.958 / 7.202 | 8,192; 7.202 / 7.446 | 8,192; 7.874 / 8.057 | 8,192; 7.690 / 7.935 |
| `repeat_1` | 512; 86.914 / 89.844 | 512; 97.656 / 101.562 | 512; 142.578 / 146.484 | 512; 134.766 / 138.672 |
| `cycle_8` | 512; 85.938 / 87.891 | 512; 99.609 / 105.469 | 512; 140.625 / 146.484 | 512; 133.789 / 138.672 |
| `cycle_9` | 512; 87.891 / 91.797 | 512; 99.609 / 107.422 | 512; 141.602 / 144.531 | 256; 220.703 / 230.469 |
| `unique_63` | 512; 87.891 / 91.797 | 512; 99.609 / 103.516 | 512; 142.578 / 146.484 | 256; 224.609 / 234.375 |

Score slot was also fastest throughout the Minecraft run. Capturing the same
score-based leaf through command return added about 12 percent to the
`repeat_1` median. On the sustained `repeat_1` and `cycle_8` workloads, macro
return was about 5 percent below storage return. Its median then rose from
133.789 µs for `cycle_8` to 220.703 µs for `cycle_9`, consistent with the
intended eight-entry macro-cache boundary; storage return stayed near 141 to
143 µs.

For this sequential singleton-frame contract, fixed score slots are therefore
the lowest-cost lowering, while score command return avoids a callee-owned
shared result slot at a measurable cost. Macro arguments are competitive only
when their specializations remain cacheable; the fixed storage frame has
higher baseline cost but predictable performance.
These conclusions do not extend to nested or reentrant calls, which require a
separate preservation and frame-allocation experiment.

Both runtime measurements use persistent warm macro caches, but their absolute
times are not paired latencies: Minecraft times batches whose calls after the
first begin with the previous public output installed, whereas Worldless
clears that output before each individually timed call. Stopwatch resolution
and runtime implementation also differ, so comparisons should remain within
each runtime.
