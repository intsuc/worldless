# Loop lowering data pack

This pack compares five lowering strategies for a fixed-count loop over signed
32-bit scoreboard values. It uses no physical-world state.

## Contract

Each public entry point reads `worldless_lab:loop_lowering/input`. The input
must contain exactly these fields:

```snbt
{iterations:<IntTag>,seed:<IntTag>}
```

`iterations` must be one of `0`, `1`, `3`, `4`, `5`, `15`, `16`, `17`, `64`,
or `256`. These are the experiment's zero/minimum cases, the boundaries around
the two partial-unroll factors, and two scale cases. `seed` may be any signed
32-bit integer. The pack round-trips both fields through scores into IntTags,
so ByteTags, LongTags, strings, missing fields, extra fields, and unsupported
iteration counts are rejected rather than coerced.

Validation uses separate scratch state. Invalid input returns failure with
value 0 before replacing calculation work state or updating public output. A
valid invocation resets all work scores before entering the selected lowering.

The public entry points are:

- `worldless_lab:loop_lowering/recursive_call/run`;
- `worldless_lab:loop_lowering/return_run/run`;
- `worldless_lab:loop_lowering/unroll_4/run`;
- `worldless_lab:loop_lowering/unroll_16/run`;
- `worldless_lab:loop_lowering/full_unroll/run`.

A successful invocation returns success with value 1 and writes these known
fields:

```snbt
{iterations:<IntTag>,value:<IntTag>,checksum:<IntTag>}
```

`iterations` is the number of loop bodies actually completed, not a copy of
the request. Before publication, the common finish function requires
`remaining == 0` and `executed == requested`. The lab clears output storage to
`{}` before every invocation, so its checked output is exactly the compound
above. Data-pack commands cannot replace a storage root compound; callers that
do not clear it retain unrelated pre-existing fields.

## Workload

Every logical iteration executes the same six scoreboard commands, in the same
order in every variant:

```text
value = wrapping_i32(value * 1664525 + 1013904223)
checksum = wrapping_i32(checksum * 31 + value)
remaining -= 1
executed += 1
```

`value` starts at `seed`, `checksum` starts at 1, `remaining` starts at the
requested count, and `executed` starts at 0. All arithmetic uses scoreboard
signed-32-bit wrapping behavior. The scalar final value and checksum make an
independent Rust oracle sufficient; no output array or storage traversal is
part of the loop body.

The timed public call includes exact input validation, score initialization,
one warmed function-macro case dispatch, the complete loop, completion checks,
and publication. Input installation, output clearing, result verification,
pack compilation, and VM construction are outside the timed interval.

## Lowerings

- `recursive_call` emits one body per function invocation. If work remains, it
  calls itself with an ordinary `function` command and later resumes the
  caller's trailing `return 1`, so parent continuations remain pending and are
  unwound.
- `return_run` has the same body and conditional call count, but its recursive
  edge is `return run function`. The caller's continuation is discarded and
  the child result is propagated.
- `unroll_4` emits the statically known remainder as a case-specific prefix,
  then tail-chains one shared function containing four bodies.
- `unroll_16` does the same with sixteen bodies per shared group function.
- `full_unroll` emits every body into a case-specific straight-line function
  and performs no loop-edge call after dispatch.

The partial-unroll group is shared across cases, so its source size is bounded
by the unroll factor. Full unrolling instead grows generated source linearly
with the supported trip counts. This isolates the compiler tradeoff between
runtime loop edges and generated data-pack size.

`return run` removes the caller continuation, but this experiment does not
instrument runtime frame depth and does not claim constant stack depth.
Worldless and the tested Minecraft server both create child function frames;
only quota, elapsed time, and generated source size are measured here.

Early exit, `continue`, dynamic trip counts, loop-carried aggregates, function
calls in the body, and scheduled multi-tick loops are separate lowering axes
and are not covered.

## Cost model and pack size

For positive `N`, measured Worldless quota follows these exact formulas:

```text
recursive_call = return_run = 68 + 8N
unroll_K                    = 68 + 6N + 2 floor(N / K)
full_unroll                 = 68 + 6N
```

All variants do `Theta(N)` arithmetic. Ordinary and return-run recursion use
`Theta(N)` loop calls. After the static prefix, a partial variant makes exactly
`floor(N / K)` shared-group calls, which is `Theta(N / K)` as `N` grows. The
full variant eliminates loop calls but generates `Theta(N)` source.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is
198,940 bytes, with 63 function resources and 2,776 nonblank command lines.
The longest command line is 197 bytes. The largest resource,
`full_unroll/n256`, is 110,601 bytes and 1,537 nonblank lines.

The following footprint counts include each variant's four-command public
wrapper but exclude the four shared functions. Bytes are source bytes for the
listed function resources.

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, dispatch, and finish | 4,839 | 4 | 52 |
| `recursive_call` | 1,312 | 12 | 22 |
| `return_run` | 1,279 | 12 | 22 |
| `unroll_4` | 6,371 | 12 | 94 |
| `unroll_16` | 20,081 | 12 | 286 |
| `full_unroll` | 164,954 | 11 | 2,300 |

`pack.mcmeta` accounts for the remaining 104 bytes. Pack loading and source
parsing are outside both runtime timing protocols, so the footprint table is a
required second axis rather than a cost already represented by elapsed time.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite loop_lowering --format text
cargo run --release -p worldless-lab -- compare --suite loop_lowering \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Iterations | Recursive call `quota; median` | Return run `quota; median` | Unroll 4 `quota; median` | Unroll 16 `quota; median` | Full unroll `quota; median` |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 68; 13.590 | 68; 13.960 | 68; 13.600 | 68; 13.720 | 68; 13.530 |
| 1 | 76; 15.710 | 76; 15.630 | 74; 15.450 | 74; 15.370 | 74; 15.270 |
| 3 | 92; 19.580 | 92; 19.570 | 86; 18.600 | 86; 18.560 | 86; 18.580 |
| 4 | 100; 21.310 | 100; 21.271 | 94; 20.380 | 92; 20.030 | 92; 20.190 |
| 5 | 108; 23.150 | 108; 23.140 | 100; 22.100 | 98; 21.690 | 98; 21.640 |
| 15 | 188; 41.790 | 188; 41.510 | 164; 38.200 | 158; 37.291 | 158; 37.350 |
| 16 | 196; 43.000 | 196; 43.260 | 172; 39.991 | 166; 39.290 | 164; 38.970 |
| 17 | 204; 45.161 | 204; 44.800 | 178; 41.790 | 172; 40.680 | 170; 40.510 |
| 64 | 580; 132.960 | 580; 130.441 | 484; 118.750 | 460; 115.160 | 452; 114.930 |
| 256 | 2,116; 524.202 | 2,116; 466.542 | 1,732; 348.761 | 1,636; 322.901 | 1,604; 321.631 |

At 256 iterations, `return_run` used exactly the same command quota as
`recursive_call` but had an 11.0 percent lower median. This is evidence that
discarding and avoiding the unwind of pending continuations matters in
Worldless even when dispatched-command count is unchanged.

Four-way unrolling reduced quota by 18.1 percent and median time by 33.5
percent relative to ordinary recursion. Sixteen-way unrolling reduced them by
22.7 and 38.4 percent. Full unrolling reduced quota by 24.2 percent, but was
only 0.4 percent faster than sixteen-way unrolling while its variant source
was about 8.2 times larger. For this body and tested bound, 16 is therefore a
more attractive default Worldless unroll factor than full expansion.

### Minecraft Java Edition

The same 10 inputs and all five public entry points were checked and measured
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

Every single-call preflight and post-batch check required return success and
value 1, exact public output, `requested == executed == N`, `remaining == 0`,
and exact final value and checksum scores. Input installation, state poisoning,
checks, and result inspection were outside the stopwatch. Calibration used the
same stopwatch batches but was excluded from the 30 reported samples. The last
call in each batch was wrapped once to capture its result before querying the
stopwatch.

The 1,024-repeat cap left every 0-through-17 median batch and the three
unrolled 64-iteration median batches below the accepted band's 50 ms lower
bound. One-millisecond quantization makes the small rows coarse; the roughly
40-to-59 ms batches at 64 iterations and the 35-to-68 ms batches at 256
iterations provide the useful runtime comparisons.

The two measured JVMs recorded 125 and 133 young collections, no full
collections, and maximum pauses of 22.841 and 24.578 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Iterations | Recursive call `R; median / p95` | Return run `R; median / p95` | Unroll 4 `R; median / p95` | Unroll 16 `R; median / p95` | Full unroll `R; median / p95` |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 1,024; 9.766 / 11.719 | 1,024; 10.254 / 11.719 | 1,024; 9.766 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 11.719 |
| 1 | 1,024; 9.766 / 10.742 | 1,024; 10.254 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 10.742 |
| 3 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 9.766 / 10.742 | 1,024; 10.254 / 10.742 | 1,024; 9.766 / 10.742 |
| 4 | 1,024; 11.719 / 12.695 | 1,024; 11.719 / 12.695 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 11.719 |
| 5 | 1,024; 11.719 / 12.695 | 1,024; 11.719 / 12.695 | 1,024; 10.742 / 12.695 | 1,024; 10.742 / 11.719 | 1,024; 10.742 / 12.695 |
| 15 | 1,024; 18.555 / 19.531 | 1,024; 18.555 / 20.508 | 1,024; 16.602 / 18.555 | 1,024; 16.602 / 17.578 | 1,024; 16.602 / 18.555 |
| 16 | 1,024; 19.531 / 20.508 | 1,024; 19.531 / 20.508 | 1,024; 17.578 / 18.555 | 1,024; 16.602 / 18.555 | 1,024; 16.602 / 18.555 |
| 17 | 1,024; 19.531 / 21.484 | 1,024; 19.531 / 21.484 | 1,024; 18.066 / 19.531 | 1,024; 18.066 / 19.531 | 1,024; 17.578 / 19.531 |
| 64 | 1,024; 51.758 / 56.641 | 1,024; 51.758 / 54.688 | 1,024; 44.922 / 47.852 | 1,024; 42.969 / 43.945 | 1,024; 43.945 / 46.875 |
| 256 | 256; 179.688 / 203.125 | 256; 181.641 / 187.500 | 256; 154.297 / 160.156 | 256; 144.531 / 171.875 | 256; 166.016 / 195.313 |

At 64 iterations, ordinary recursion and return-run had the same median. At
256, return-run was 1.1 percent slower. The tested Minecraft runtime therefore
showed no timing benefit from return-run alone for this small score-only body,
despite its different continuation semantics.

At 256 iterations, four-way and sixteen-way unrolling were 14.1 and 19.6
percent faster than ordinary recursion. Full unrolling was only 7.6 percent
faster than recursion and was 14.9 percent slower than sixteen-way unrolling,
despite using 32 fewer commands. The 110 KB straight-line function therefore
crossed a useful code-size/runtime boundary: fewer dispatched commands did not
mean lower latency. Sixteen-way unrolling is the best tested compiler default
for this body on both runtimes, with full unrolling reserved for much smaller
constant loops if code size permits.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock; Worldless measures individual calls with its host clock.
Runtime implementation, batching, GC, and order controls differ, so
comparisons should remain within each runtime.

## Failure boundaries

Disposable Worldless checks covered either or both fields missing, an extra
field, ByteTag and LongTag inputs, counts -1, 2, 6, and 257, and every public
variant. Disposable Minecraft checks additionally covered string types for
both fields. Every invalid call returned failure with value 0 while distinct
sentinels in all five calculation scores and public output remained unchanged.
A valid call on the same VM/server then recovered normally.

Minimum, zero, and maximum signed-32-bit IntTag seeds succeeded with exact
oracle output and internal scores for all five variants in both runtimes. All
generated body files were also mechanically checked as repetitions of the
canonical six-command block.

For each 256-iteration variant, a Worldless invocation with the exact measured
quota `Q` reported `CommandLimitExceeded`, while `Q + 1` succeeded with exact
state and output. The respective `Q` values were 2,116, 2,116, 1,732, 1,636,
and 1,604.

At limit 100, calculation state was partially updated while old public output
remained unchanged. Rerunning at the full limit on the same VM succeeded
because a valid entry replaces work before dispatch. A separate Minecraft JVM
repeated the limit-100 and recovery probe for every variant: the server stopped
after exactly 100 commands, the outer return sentinel remained -999, public
output stayed unchanged, and six to eight loop iterations were partially
represented in scores. Restoring the limit to 1,000,000 made every preflight
succeed.

Loop iterations and publication are not transactional. A command limit can
interrupt between `remaining -= 1` and `executed += 1`, or during publication;
apparently complete state or output at a quota boundary does not establish
success. The execution report is authoritative.

## Limits

The result supports a lowering heuristic only for this six-command arithmetic
body, fixed counts no greater than 256, warmed macro dispatch, and the tested
runtime versions. It does not measure data-pack load latency, parsed command
memory, runtime frame memory, dynamic loop conditions, nested loops, or body
costs large enough to amortize loop edges differently. A compiler should keep
unroll factor and full-unroll thresholds explicit and remeasure them when its
generated body shape or target runtime changes.
