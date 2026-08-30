# Dynamic vector data pack

This pack compares three storage representations for a bounded, dynamically
indexed vector of signed 32-bit integers without physical-world state.

## Contract

Each public entry point reads `worldless_lab:dynamic_vector/input`. The input
must contain exactly these fields:

```snbt
{length:<IntTag>,seed:<IntTag>,workload:"build"|"random_update"|"churn"}
```

`length` must be between 0 and 256 inclusive. `random_update` additionally
requires a nonzero length. `seed` may be any signed 32-bit integer. Invalid
input returns failure with value 0 before replacing internal work state or
updating public output. Validation round-trips both numeric fields through
scores into IntTags and compares them with the original input, so other
numeric tag types are not accepted through numeric coercion.

The public entry points are:

- `worldless_lab:dynamic_vector/primitive_append/run`;
- `worldless_lab:dynamic_vector/preallocated/run`;
- `worldless_lab:dynamic_vector/chunked_16/run`.

A successful invocation sets `length` and `checksum` in
`worldless_lab:dynamic_vector/output` and returns success with value 1. The lab
clears output storage before every invocation, so its checked output is exactly
`{length:<IntTag>,checksum:<IntTag>}`.

## Representations

All variants expose the same zero-based logical vector. Its length is held in
a score while the invocation runs.

- `primitive_append` starts with an empty IntArray, appends each pushed value,
  and physically removes the last element on pop. Its physical and logical
  lengths are always equal.
- `preallocated` creates one zero-filled IntArray of length 256. Push writes at
  the logical tail through a function macro. Pop changes only the logical
  length; later pushes overwrite inactive slots.
- `chunked_16` stores a list of zero-filled 16-element IntArrays. Push appends a
  page only when logical length equals capacity, then writes through a nested
  macro path. Pop changes only logical length, and pages are retained for
  reuse. The physical page count is `ceil(maximum_reached_length / 16)`.

The common generator emits the current state and then advances it with signed
32-bit wrapping arithmetic:

```text
value = state
state = wrapping_i32(state * 1664525 + 1013904223)
```

No variant receives a pre-encoded vector as input.

## Workloads

Every workload first pushes `length` generated values. It then performs one of
these traces:

- `build`: no further mutation;
- `random_update`: 63 times, use the current generator state modulo logical
  length as an index, advance the generator, read the selected value, and write
  `wrapping_i32(value * 31 + 7)` back. Negative remainders are corrected to
  Euclidean modulo before indexing;
- `churn`: pop `floor(length / 2)` tail values, then push the same number of
  newly generated values, preserving the original logical length.

Finally, all logical elements are read sequentially and folded from 1:

```text
checksum = wrapping_i32(checksum * 31 + value)
```

The timed boundary is therefore an end-to-end public call: validation,
allocation, generation, mutation, the final full-vector scan, and publication.
In particular, `build` is not a push-only microbenchmark. Flat variants share
the same dynamically indexed final scan, while `chunked_16` also pays page and
offset arithmetic plus a nested macro path. Timing differences must not be
attributed solely to growth.

The registered cases cover lengths 0 and 1, both sides of the 16-element page
boundary, and scales 64 and 256. `build` uses 0, 1, 15, 16, 17, 64, and 256;
`random_update` uses 1, 16, 17, 64, and 256; `churn` uses 15, 16, 17, 64, and
256.

## Cost model

Each representation has linear command count for initial construction and the
final scan, but the native storage work differs. Worldless backs IntArrays with
a growable vector, so repeated `primitive_append` growth is amortized linear.
Minecraft's current primitive-array append replaces and copies the backing
array, making the total native copy volume quadratic in the number of pushes
even though command quota remains linear.

`preallocated` avoids resize copies, but pays for a 256-element literal on every
call and a dynamically specialized macro write on every push. `chunked_16`
limits each allocation to 16 integers, at the cost of quotient/remainder
arithmetic and nested paths on every indexed access. Dynamic macro paths also
interact with the runtimes' small specialization caches; persistent warm state
does not make an unbounded sequence of distinct indices permanently cached.

Command quota counts dispatched commands, not bytes copied by a native NBT
operation. Quota is consequently a measured cost rather than an equality
condition across variants.

## Pack size

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is 14,505
bytes and contains 36 function resources with 168 nonblank command lines. The
longest line is the 686-byte zero-filled preallocation command. Pack loading,
source parsing, and macro compilation before invocation are outside the
Worldless timings; Minecraft uses persistent server state and exercises macro
specialization during warm-up and measurement.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite dynamic_vector --format text
cargo run --release -p worldless-lab -- compare --suite dynamic_vector \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Workload | Length | Primitive append `quota; median` | Preallocated `quota; median` | Chunked 16 `quota; median` |
| --- | ---: | ---: | ---: | ---: |
| `build` | 0 | 68; 14.060 | 68; 13.931 | 68; 13.800 |
| `build` | 1 | 97; 21.590 | 99; 22.020 | 118; 27.010 |
| `build` | 15 | 503; 286.851 | 533; 376.931 | 790; 376.621 |
| `build` | 16 | 532; 232.491 | 564; 337.081 | 838; 395.371 |
| `build` | 17 | 561; 242.940 | 595; 354.271 | 888; 418.201 |
| `build` | 64 | 1,924; 709.283 | 2,052; 1,120.673 | 3,148; 1,348.334 |
| `build` | 256 | 7,492; 2,630.128 | 8,004; 4,238.544 | 12,388; 5,175.797 |
| `random_update` | 1 | 1,989; 398.352 | 1,991; 396.811 | 2,766; 588.252 |
| `random_update` | 16 | 2,424; 1,289.624 | 2,456; 1,368.475 | 3,486; 1,615.785 |
| `random_update` | 17 | 2,453; 939.473 | 2,487; 1,040.343 | 3,536; 1,291.545 |
| `random_update` | 64 | 3,816; 1,772.806 | 3,944; 2,194.738 | 5,796; 2,619.469 |
| `random_update` | 256 | 9,384; 3,676.382 | 9,896; 5,312.177 | 15,036; 6,427.192 |
| `churn` | 15 | 678; 259.171 | 701; 360.011 | 1,035; 428.112 |
| `churn` | 16 | 731; 273.481 | 755; 382.262 | 1,117; 453.831 |
| `churn` | 17 | 760; 282.561 | 786; 398.731 | 1,167; 475.171 |
| `churn` | 64 | 2,699; 859.543 | 2,795; 1,467.165 | 4,243; 1,759.655 |
| `churn` | 256 | 10,571; 3,196.561 | 10,955; 5,578.809 | 16,747; 6,741.942 |

For every nonempty row, `primitive_append` used the least quota. It also had
the lowest median except for a 0.4 percent reversal against preallocation on
length-1 `random_update`. At length 256 it was 31 to 43 percent faster than
preallocation across the three workloads. `chunked_16` was 75 to 111 percent
slower than primitive append. This result favors direct IntArray append for the
tested bounded i32 vector lowering in Worldless.

### Minecraft Java Edition

The same 17 inputs and all three public entry points were checked and measured
on Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31. The host
was the same Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The server used
Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players. The
server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` complete public calls. Calibration selected a power-of-two `R`, up to
1,024, toward 100 ms while keeping `R` times the Worldless quota below 800,000.
Each of two fresh JVMs discarded 20 warm-up batches and measured 15 batches per
row; the second JVM traversed all rows in reverse order. The table pools the 30
unfiltered batch averages. Each cell is `R; median / nearest-rank p95`, in
microseconds per call.

Every single-call preflight and each post-batch check required return value 1
and exact public output. They also checked exact physical layouts and values:
an IntArray of logical length for primitive append, an IntArray of length 256
for preallocation, and `ceil(length / 16)` IntArray pages of length 16 for the
chunked representation. Input installation, calibration, checks, and result
inspection were outside the stopwatch.

The two measured JVMs recorded 506 and 504 young collections, no full
collections, and maximum pauses of 50.196 and 45.333 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Workload | Length | Primitive append `R; median / p95` | Preallocated `R; median / p95` | Chunked 16 `R; median / p95` |
| --- | ---: | ---: | ---: | ---: |
| `build` | 0 | 1,024; 9.766 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 9.766 / 10.742 |
| `build` | 1 | 1,024; 12.695 / 13.672 | 1,024; 12.695 / 13.672 | 1,024; 14.648 / 15.625 |
| `build` | 15 | 512; 92.773 / 99.609 | 1,024; 136.719 / 137.695 | 512; 167.969 / 169.922 |
| `build` | 16 | 512; 97.656 / 99.609 | 1,024; 145.508 / 146.484 | 512; 177.734 / 181.641 |
| `build` | 17 | 512; 103.516 / 107.422 | 1,024; 153.320 / 155.273 | 512; 187.500 / 193.359 |
| `build` | 64 | 256; 359.375 / 367.188 | 128; 546.875 / 554.688 | 128; 671.875 / 695.312 |
| `build` | 256 | 64; 1,414.062 / 1,453.125 | 32; 2,156.250 / 2,187.500 | 32; 2,671.875 / 2,750.000 |
| `random_update` | 1 | 256; 273.438 / 281.250 | 256; 275.391 / 285.156 | 256; 375.000 / 382.812 |
| `random_update` | 16 | 128; 628.906 / 640.625 | 128; 671.875 / 679.688 | 64; 812.500 / 828.125 |
| `random_update` | 17 | 128; 492.188 / 507.812 | 128; 539.062 / 554.688 | 128; 679.688 / 695.312 |
| `random_update` | 64 | 64; 890.625 / 921.875 | 64; 1,078.125 / 1,093.750 | 64; 1,320.312 / 1,359.375 |
| `random_update` | 256 | 32; 1,953.125 / 2,000.000 | 32; 2,687.500 / 2,750.000 | 16; 3,312.500 / 3,437.500 |
| `churn` | 15 | 512; 113.281 / 115.234 | 512; 162.109 / 164.062 | 256; 199.219 / 207.031 |
| `churn` | 16 | 512; 123.047 / 126.953 | 512; 171.875 / 177.734 | 256; 214.844 / 222.656 |
| `churn` | 17 | 512; 128.906 / 132.812 | 512; 179.688 / 185.547 | 256; 226.562 / 234.375 |
| `churn` | 64 | 128; 457.031 / 476.562 | 128; 726.562 / 742.188 | 64; 898.438 / 937.500 |
| `churn` | 256 | 32; 1,796.875 / 1,875.000 | 32; 2,875.000 / 2,937.500 | 16; 3,625.000 / 4,062.500 |

Primitive append again had the lowest nontrivial median, tying preallocation
only for `build` at length 1. At length 256 it was 27 to 38 percent faster than
preallocation across workloads, while `chunked_16` was 70 to 102 percent
slower than primitive append. Up to the tested bound, avoiding one
macro-indexed write per push outweighed Minecraft's primitive-array copy cost.
That does not remove the quadratic native copy model or establish where a
crossover occurs beyond length 256.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock, while Worldless measures individual invocations using its
host clock. Runtime implementation, batching, and row-order controls also
differ, so comparisons should remain within each runtime.

## Failure boundaries

Disposable checks covered every missing field, an extra field, wrong field
types, lengths -1 and 257, an unknown workload, and zero-length random update.
Each invalid call returned 0 without changing sentinel work or public output.
Minimum, zero, and maximum signed-32-bit IntTag seeds succeeded, and a valid
call after invalid input recovered normally. The zero-seed check exercised an
unchanged write into zero-filled preallocated and chunked slots; missing
dynamic paths returned failure instead of being read as zero.

The exact physical layout and every active value were checked for all 51
case-variant invocations, including lengths 15, 16, and 17 around the page
boundary. Inactive preallocated slots and unused page tails remained zero for
the registered traces.

The largest row, length-256 `churn` with `chunked_16`, used Worldless quota
16,747. A direct invocation with that exact limit reported
`CommandLimitExceeded`; 16,748 succeeded. At limit 100, internal work was only
partially constructed while old public output remained unchanged. A
subsequent full-limit invocation on the same VM succeeded because validation
completed and the public entry point replaced work before running the trace.
Execution status, rather than apparently complete storage, is authoritative at
a quota boundary.

A separate fresh Minecraft JVM first checked the zero-seed call and exact
physical layout for all three variants, then ran the same largest row with an
empirical command sequence limit of 100. The server reported stopping after
exactly 100 commands; the surrounding `execute store` retained its return
sentinel of -999, expected public output was absent, and one partial page was
present. Restoring the
normal limit of 1,000,000 and rerunning the complete preflight on the same JVM
succeeded. This probe was not mixed into calibration, warm-up, timing, or GC
results.

## Limits of the conclusion

This experiment covers a per-call rebuilt vector with a maximum length of 256,
homogeneous IntTag values, tail push/pop, 63 point updates, and a final full
scan. It does not cover larger vectors, persistent mutation across public
calls, arbitrary operation traces, middle insertion or deletion, alternative
page sizes, or compound and heterogeneous elements. The result supports a
compiler lowering decision only inside that measured envelope.
