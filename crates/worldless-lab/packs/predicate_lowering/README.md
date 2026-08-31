# Predicate lowering data pack

This pack compares four ways to lower a pure conjunction of 16 canonical
score booleans. It fixes input loading, evaluation count, result capture, and
checksum work, then varies only the representation of the predicate itself.

## Contract

Each public entry point reads `worldless_lab:predicate_lowering/input`. The
input must contain exactly one field:

```snbt
{terms:[I; <exactly 16 values>]}
```

Every element must be the integer 0 or 1. Missing or extra fields, another NBT
type, another array length, and other integer values are rejected rather than
coerced to truth values.

Validation uses separate scratch storage and scores. Invalid input returns
failure with value 0 before replacing calculation work, changing the 16 term
scores or other calculation scores, or updating public output. A valid call
copies the terms into fixed holders `#t0` through `#t15` before evaluation.

The public entry points are:

- `worldless_lab:predicate_lowering/execute_chain/run`;
- `worldless_lab:predicate_lowering/guard_return/run`;
- `worldless_lab:predicate_lowering/score_product/run`;
- `worldless_lab:predicate_lowering/predicate_resource/run`.

A successful invocation returns success with value 1 and publishes:

```snbt
{result:<IntTag>,checksum:<IntTag>}
```

The common finish function requires `result` to be 0 or 1 and all 63
evaluations to have completed before publication. The lab clears output
storage to `{}` before each call, so its checked output is exactly the compound
above. Data-pack commands cannot replace a storage root compound; callers that
do not clear it retain unrelated pre-existing fields.

## Workload

Each invocation evaluates the same 16 terms 63 times. The caller captures the
scalar result of every evaluation and folds it into a signed wrapping-i32
checksum:

```text
checksum = 1
repeat 63 times:
  result = terms[0] && ... && terms[15]
  checksum = checksum * 31 + result
```

The driver statically emits all 63 evaluator calls and folds. There is no
runtime loop, recursion, macro, or dynamic score target in the measured
evaluation path. A warmed function macro is used only while validating the
input array.

The five registered cases isolate the first decisive false term:

| Case | False term | Terms visited by a short-circuit evaluation |
| --- | ---: | ---: |
| `false_0` | 0 | 1 |
| `false_4` | 4 | 5 |
| `false_8` | 8 | 9 |
| `false_15` | 15 | 16 |
| `all_true` | none | 16 |

Every false case has exactly one zero; all other terms are one. Thus
`false_15` and `all_true` have the same maximum lookup depth while separating
the terminal false and true paths. False rows produce
`{result:0,checksum:2120287199}`; `all_true` produces
`{result:1,checksum:329810944}`.

The timed public call includes validation, term loading, all 63 evaluations
and captures, checksum folding, completion checks, and publication. Input
installation, output clearing, result inspection, pack compilation, and VM
construction are outside the timer.

## Lowerings

- `execute_chain` keeps the conjunction in condition context. One command has
  16 ordered `execute if score` modifiers and returns 1 when all pass; the
  following command returns 0.
- `guard_return` emits one ordered `execute unless score ... return 0` guard
  per term, followed by `return 1`. A decisive false prevents later guard
  commands from running.
- `score_product` assigns term zero to `#bool`, multiplies it by terms 1
  through 15, and returns that score. Exact 0/1 validation makes multiplication
  equivalent to conjunction, but every term is evaluated eagerly.
- `predicate_resource` moves the conjunction into a static `minecraft:all_of`
  predicate resource containing 16 ordered score-backed `value_check` terms.
  Its two-command evaluator returns 1 or 0 from that resource.

All four forms use the same fixed term holders and order, evaluator function
boundary, command-result capture, fold function, completion counter, and
publication path. The terms are pure equal-cost score reads, so short-circuit
position is the only data-dependent axis.

## Cost model and pack size

For Worldless, the exact quota is:

```text
Q = 125 + 63p

execute_chain:      p = 22
guard_return false: p = 7 + first_false_index
guard_return true:  p = 22
score_product:      p = 23
predicate_resource: p = 7
```

The fixed 125 commands cover common validation, setup, dispatch, and finish.
Each evaluation also includes the shared call-result capture and four-command
fold path. Worldless charges all modifiers in the fused execute chain even
after the chain becomes inactive, although later score lookups are skipped.
The predicate resource's internal provider nodes are not dispatched commands,
so they do not consume Worldless command quota; their evaluation still appears
in elapsed time.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is
48,601 bytes across 18 resources: 16 function resources, one predicate
resource, and `pack.mcmeta`. It contains 661 nonblank command lines. The
longest command is the 570-byte execute chain, while the predicate JSON is
2,412 bytes.

| Portion | Bytes | Resources | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, fold, and finish | 8,321 | 4 functions | 103 |
| `execute_chain` wrapper, driver, evaluator | 9,283 | 3 functions | 132 |
| `guard_return` wrapper, driver, evaluator | 9,598 | 3 functions | 147 |
| `score_product` wrapper, driver, evaluator | 9,783 | 3 functions | 147 |
| `predicate_resource` wrapper, driver, evaluator | 9,095 | 3 functions | 132 |
| `predicate_resource` JSON | 2,412 | 1 predicate | 0 |

`pack.mcmeta` accounts for the remaining 109 bytes. Pack loading, JSON parsing,
and parsed-resource memory are outside both timing protocols, so generated
size remains a separate compiler tradeoff.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite predicate_lowering --format text
cargo run --release -p worldless-lab -- compare --suite predicate_lowering \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and rows are traversed in fixed table order.

| Case | Execute chain `quota; median` | Guard return `quota; median` | Score product `quota; median` | Predicate resource `quota; median` |
| --- | ---: | ---: | ---: | ---: |
| `false_0` | 1,511; 86.960 | 566; 82.921 | 1,574; 323.551 | 566; 88.070 |
| `false_4` | 1,511; 95.601 | 818; 101.801 | 1,574; 323.761 | 566; 100.230 |
| `false_8` | 1,511; 104.820 | 1,070; 118.511 | 1,574; 324.891 | 566; 112.810 |
| `false_15` | 1,511; 119.920 | 1,511; 150.301 | 1,574; 324.061 | 566; 133.091 |
| `all_true` | 1,511; 117.731 | 1,511; 152.740 | 1,574; 324.191 | 566; 132.541 |

The execute chain's quota was constant, but its median rose 37.9 percent from
`false_0` to `false_15`, consistent with skipped score lookups still affecting
elapsed work even when modifier quota is fixed. Guard returns saved both quota
and 4.6 percent median time for `false_0`; by `false_4`, their extra command
boundaries outweighed the saved lookups in this runtime.

The predicate resource also held quota constant at 566, 62.5 percent below the
execute chain. Its median still changed with decision depth and was 12.6
percent above the execute chain on `all_true`. Eager score multiplication used
only 4.2 percent more quota than the execute chain but took roughly 2.7 to 3.7
times as long in these rows. Quota therefore does not model the relative cost
of score arithmetic, condition evaluation, and predicate-provider evaluation.

### Minecraft Java Edition

The same matrix was checked and measured on Minecraft Java dedicated server
`26.3-snapshot-10` on 2026-08-31. The host was the same Ryzen 9 9950X3D
running Linux 7.0.0-29-generic. The server used Microsoft OpenJDK
25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players. The server JAR
SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `R` complete public calls. Calibration selected a power-of-two `R`, up to
1,024, toward 100 ms while keeping `R` times Worldless quota below 800,000.
Each of two fresh JVMs discarded 20 warm-up batches and measured 15 batches
per row; the second JVM traversed the complete matrix in reverse. Calibration
batches used the same stopwatch but are not among the 30 pooled, unfiltered
samples. Each cell is `R; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight and post-batch check required return success and
value 1, exact public and internal results, exact term and validation layouts,
the independent result and checksum, and exactly 63 completed evaluations.
Input installation, state poisoning, checks, and score retrieval were outside
the stopwatch.

The measured JVMs recorded 124 and 126 young collections, no full
collections, and maximum pauses of 23.580 and 49.361 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Case | Execute chain `R; median / p95` | Guard return `R; median / p95` | Score product `R; median / p95` | Predicate resource `R; median / p95` |
| --- | ---: | ---: | ---: | ---: |
| `false_0` | 512; 52.734 / 54.688 | 1,024; 44.922 / 52.734 | 256; 136.719 / 144.531 | 1,024; 57.617 / 58.594 |
| `false_4` | 512; 60.547 / 62.500 | 512; 58.594 / 62.500 | 256; 140.625 / 144.531 | 1,024; 61.523 / 64.453 |
| `false_8` | 512; 68.359 / 70.312 | 512; 74.219 / 78.125 | 256; 136.719 / 140.625 | 1,024; 68.359 / 70.312 |
| `false_15` | 512; 83.984 / 87.891 | 512; 99.609 / 101.562 | 256; 136.719 / 144.531 | 1,024; 74.219 / 76.172 |
| `all_true` | 512; 83.984 / 85.938 | 512; 120.117 / 140.625 | 256; 140.625 / 152.344 | 1,024; 74.219 / 80.078 |

Minecraft showed the same decision-depth response but a different crossover.
Guard returns were 14.8 percent faster than the execute chain at `false_0`
and 3.2 percent faster at `false_4`; they were 8.6 percent slower by
`false_8` and 43.0 percent slower on `all_true`. The execute-chain median rose
59.3 percent from `false_0` to `false_15` despite constant Worldless quota.

The predicate resource was slower than guards for the two earliest failures,
tied the execute-chain median at `false_8`, and was 11.6 percent faster than
the execute chain on both maximum-depth rows. Score multiplication remained
the slowest lowering in every Minecraft row. These timings support a
runtime-specific short-circuit heuristic rather than one ranking derived from
command count alone.

Worldless and Minecraft both used persistent warm state, but their absolute
times are not paired latencies. Minecraft measures batches in a server JVM
with a 1 ms clock; Worldless measures individual calls with its host clock.
Runtime implementation, batching, GC, and row-order controls differ, so
comparisons should remain within each runtime.

## Failure boundaries

Disposable Worldless and Minecraft checks each covered 20 invalid shapes for
all four variants. The matrix included a missing or extra field, 11 wrong
scalar or container tag shapes, array lengths 0, 15, and 17, and values -1 or
2 at both the first and last positions. All 80 invocations per runtime returned
failure with value 0 while public output, calculation work, and calculation
scores retained distinct sentinels.

Separate success probes placed the only false value at every index from 0
through 15 and also checked `all_true`. All 68 invocations per runtime matched
the independent oracle, which verifies every generated term reference rather
than relying only on the five timed positions.

For every one of the 20 Worldless rows, an invocation with its exact measured
quota `Q` reported `CommandLimitExceeded`, while `Q + 1` succeeded with exact
state and output on the same VM. The observed `Q` values were 1,511 for the
execute chain, 566/818/1,070/1,511/1,511 for guards, 1,574 for the score
product, and 566 for the predicate resource.

At limit 256, both runtimes interrupted after six completed evaluations for
the execute chain, guard, and score product, and after 20 for the predicate
resource. Preparation and term loading had completed, but public output and
the outer Minecraft return capture retained their old sentinels. Restoring the
normal limit, clearing only public output, and invoking again on the same VM or
server produced exact state and output for all four variants; no external
reset of the partial calculation state was performed.

Function execution and publication are not transactional. Command-limit
status is authoritative even if some scores appear complete near a boundary;
a generated-language runtime must reset or stage state explicitly after
interruption.

## Compiler implications and limits

For this pure conjunction, an explicit guard CFG is useful only when failures
are expected very early: its quota grows directly with the first-false index,
and the extra commands become costly on deep or true paths. A fused execute
chain is compact and avoids those command boundaries, but its command quota
does not fall with short-circuit depth.

A static predicate resource is an effective hoisting option when the target
runtime supports it. It had the smallest and depth-independent Worldless quota
and the best Minecraft median for maximum-depth evaluation, at the cost of an
additional JSON resource and a runtime ranking that differed in Worldless.
Eager multiplication is a poor control-flow lowering here; it may still be
appropriate when a materialized boolean is reused, which this experiment does
not test.

The experiment fixes a 16-term AND, canonical score booleans, pure equal-cost
reads, 63 repetitions of one fixed vector, and five static decision depths. It
does not cover OR, NOT, mixed or nested trees, truthiness conversion,
side-effecting or failing terms, function or storage-read terms, heterogeneous
term costs, mixed selectivity, branch-history distributions, dynamic term
counts, macros, reentrancy, entities, world state, random or contextual
predicates, or line-length scaling beyond 16 terms. Data-pack load latency,
JSON parse cost, parsed-resource memory, and CPU branch profiling are also
unmeasured.
