# Call frame data pack

This pack compares three ways to preserve caller-local integer values across
nested, reentrant function calls without physical-world state.

## Contract

Each public entry point reads `worldless_lab:call_frames/input`. The input
must contain exactly these fields:

```snbt
{depth:<IntTag>,seeds:[I;...31 integers...]}
```

`depth` must be between 1 and 16 inclusive. Invalid input fails before the
pack replaces its work state or updates the public output.

For signed 32-bit wrapping arithmetic, the recursive function is:

```text
F(n, x):
  left = x * 17 + n
  right = x * 31 - n
  if n == 1:
    return left * 31 + right + 7
  child = F(n - 1, x + left)
  return child * 31 + (left if child < 0 else right)
```

The pack calls `F(depth, seed)` for all 31 seeds in order and folds the
results into a checksum starting at 1:

```text
checksum = checksum * 31 + result
```

A successful invocation verifies that both internal stacks are empty, sets the
`checksum` field in `worldless_lab:call_frames/output`, and returns success
with value 1. The lab clears that output storage before each invocation, so its
checked output is exactly `{checksum:<IntTag>}`.

The public entry points are:

- `worldless_lab:call_frames/static_scores/run`;
- `worldless_lab:call_frames/word_stack/run`;
- `worldless_lab:call_frames/compound_stack/run`.

All variants pass the recursive arguments through shared scores and return the
result through the function command value. `static_scores` assigns distinct
score holders to each of the 16 supported call levels. `word_stack` spills
the two caller-live values to a flat integer array. `compound_stack` spills
the same values as one compound frame. Only the preservation strategy differs;
the dynamic variants call the same function resource reentrantly.

Here, reentrant means that the same function resource becomes active again
before its caller activation returns. Function execution remains synchronous;
the experiment does not model concurrent VM invocations.

## Workloads and measurement

The Rust suite owns the workload values. It generates 31 seeds: even index
`i` uses `i32::MIN + i`, and odd `i` uses `i32::MAX - i`. Every case uses the
same seeds and changes only the call depth: 1, 2, 4, 8, or 16. Depth 1 needs no
spill, depth 2 is the smallest overlapping activation, and the larger depths
show the scaling of frame preservation.

The static implementation pays a code-size cost for its bounded runtime: its
recursive portion contains 16 level functions, 16 level-specific base
functions, 32 local score holders, and 313 nonblank commands. Including its
driver gives 437 commands. Each dynamic implementation has one recursive
function and uses the shared five-command base; including its driver gives 152
commands for word stack and 150 for compound stack.

Persistent comparison creates one VM per case-and-variant row, discards the
requested warm-up invocations, and then measures repeated invocations on that
same VM. Rows do not share VM state. Each timed invocation includes validation,
stack initialization, all 31 recursive root calls, checksum folding, stack
balance checks, and output publication. Pack loading, VM creation, input
installation, output clearing, and result verification remain outside the
timer.

From the repository root, run correctness and persistent comparison with:

```sh
cargo run -p worldless-lab -- check --suite call_frames --format text
cargo run --release -p worldless-lab -- compare --suite call_frames \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Sample results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in the fixed table order.

| Depth | Static scores `quota; median` | Word stack `quota; median` | Compound stack `quota; median` |
| ---: | ---: | ---: | ---: |
| 1 | 702; 167.810 | 702; 167.031 | 702; 166.711 |
| 2 | 1,213; 300.851 | 1,585; 369.761 | 1,523; 364.992 |
| 4 | 2,236; 572.062 | 3,352; 761.252 | 3,166; 573.332 |
| 8 | 4,283; 882.053 | 6,887; 1,210.434 | 6,453; 1,168.624 |
| 16 | 8,368; 1,788.506 | 13,948; 2,425.077 | 13,018; 2,371.348 |

There are `31 * (depth - 1)` nested call edges. Relative to depth 1, quota
grew by about 16.5 commands per edge for static scores, 28.5 for the word
stack, and 26.5 for the compound stack. The dynamic preservation work therefore
added about 12 commands per edge for the two independent words and 10 for the
single compound frame. At depth 16, static scores had the lowest median;
compound stack was about 2.2 percent below word stack. The static result must
be weighed against its generated functions and score slots rather than treated
as a general unbounded-frame implementation.

### Minecraft Java Edition

The same inputs and entry points were checked and measured on Minecraft Java
dedicated server `26.3-snapshot-10` on 2026-08-31. The host was the same Ryzen
9 9950X3D running Linux 7.0.0-29-generic. The server used Microsoft OpenJDK
25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players. The server JAR
SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` consecutive public calls. Calibration selected a power-of-two `R`
toward 100 ms while keeping `R` times the Worldless quota below 800,000. Some
rows remained below the preferred 50 ms batch floor because this quota cap was
binding. For every row, each of two fresh JVMs discarded 20 warm-up batches
and measured 15; the second JVM traversed all rows in reverse order. The table
pools the resulting 30 unfiltered batch averages. Each cell is
`R; median / nearest-rank p95`, in microseconds per call.

Input installation, calibration, the single-call preflight, and all result
checks were outside the stopwatch. Every preflight returned 1, produced
exactly the expected output without extra fields, and left both internal
stacks present and empty. Every measured batch ended with the expected output
and both stacks empty. The two JVMs recorded 108 and 106 young collections,
no full collection, and maximum pauses of 23.200 and 49.428 ms. Samples were
not filtered, so pauses inside timed batches remain in the results.

| Depth | Static scores `R; median / p95` | Word stack `R; median / p95` | Compound stack `R; median / p95` |
| ---: | ---: | ---: | ---: |
| 1 | 1,024; 67.383 / 71.289 | 1,024; 66.406 / 68.359 | 1,024; 65.430 / 68.359 |
| 2 | 512; 112.305 / 115.234 | 256; 169.922 / 179.688 | 512; 158.203 / 162.109 |
| 4 | 256; 203.125 / 210.938 | 128; 367.188 / 382.812 | 128; 335.938 / 351.562 |
| 8 | 128; 390.625 / 406.250 | 64; 765.625 / 781.250 | 64; 703.125 / 718.750 |
| 16 | 64; 765.625 / 796.875 | 32; 1,531.250 / 1,812.500 | 32; 1,375.000 / 1,625.000 |

Depth 1 performs no spill, and the three medians there are within stopwatch
quantization and run-order noise. From depth 2 onward, static scores were the
fastest. Compound stack was consistently faster than word stack on Minecraft;
at depth 16 its median was about 10.2 percent lower. For this two-word frame,
one structured push and pop therefore beat two independent primitive-array
words. The experiment does not establish the crossover for wider frames,
heterogeneous locals, full argument frames, or tail calls.

Both runtimes used persistent warm state, but their absolute times are not
paired latencies. Minecraft measures batches in a server JVM with a 1 ms
clock, while Worldless measures individual invocations with its host clock;
the runtime implementations and row-order controls also differ. Comparisons
should remain within each runtime.

## Command-limit behavior

Input validation was checked separately with extra fields, wrong NBT types,
wrong seed lengths, and depths outside the supported range. Those failures
returned 0 without changing pre-existing work or output state.

Quota interruption is different: neither runtime rolls mutations back. The
largest Worldless row, depth-16 word stack, used quota 13,948 under the normal
limit. Re-running with a limit of 13,948 reported `CommandLimitExceeded` even
though the checksum had been published and both stacks were empty; 13,949
succeeded. At a lower limit of 7,000, interruption left 28 words on the stack
and preserved the old output. A depth-16 compound call interrupted at 6,500
left 12 frames, including a partially initialized top frame. In both cases, a
subsequent depth-1 call on the same VM succeeded because validation completed
and `prepare` replaced the prior work state.

A separate fresh Minecraft JVM ran depth-16 compound stack with an empirical
command-sequence limit of 100. Minecraft reported that it stopped after 100
commands; the return consumer retained its sentinel, the output was not valid,
and four frames remained. Restoring the normal limit of 1,000,000 allowed the
same JVM to pass a complete recovery preflight. This probe was not mixed into
the timing samples.

Consequently, command-limit status is authoritative even if output or return
feedback appears complete near a boundary. A generated language runtime should
treat quota exhaustion as an aborted invocation that requires explicit reset
or transactional publication, not as exception unwinding that automatically
pops its data-pack frames.
