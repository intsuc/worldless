# Scalar replacement data pack

This pack measures when a compiler should keep fixed-shape IntArray elements
in scoreboard values instead of loading and storing every element around each
use. It uses command storage and logical scores without physical-world state.

## Contract

Each public entry point reads `worldless_lab:scalar_replacement/input`. The
input must contain exactly these fields:

```snbt
{width:<IntTag>,rounds:<IntTag>,seed:<IntTag>}
```

`width` must be 1, 4, 8, or 16; `rounds` must be 1, 4, or 16; and `seed` may
be any signed 32-bit integer. Validation copies the request, removes the three
known fields, and requires the remainder to be empty. It also round-trips all
three values through scores into IntTag fields and uses an exact NBT partial
match, so other numeric tag types are not accepted through numeric coercion.

The public entry points are:

- `worldless_lab:scalar_replacement/storage_roundtrip/run`;
- `worldless_lab:scalar_replacement/score_cached/run`;
- `worldless_lab:scalar_replacement/hot_4_cache/run`.

Invalid input returns failure with value 0 before replacing internal work or
updating public output. A successful invocation materializes the complete
final IntArray in internal storage, writes `checksum` to
`worldless_lab:scalar_replacement/output`, and returns success with value 1.
The lab clears output before each checked invocation, so the checked output is
exactly `{checksum:<IntTag>}`.

## Workload

Every valid invocation creates a flat IntArray of `width` elements. The input
seed is the first value; later values use this signed 32-bit wrapping generator:

```text
state = wrapping_i32(state * 1664525 + 1013904223)
```

Each logical round visits the elements in ascending index order and applies:

```text
value = wrapping_i32(value * 31 + 7)
```

After all rounds, a common function reloads every final value from storage and
folds it into a checksum starting at 1:

```text
checksum = wrapping_i32(checksum * 31 + value)
```

Reading the checksum from the common materialized IntArray makes write-back a
required part of every cached strategy. Initialization, validation, dispatch,
the update rounds, write-back, checksum, and publication are inside each timed
public call. Input installation, output clearing, result inspection, pack
loading, and parsing are outside it.

The generated drivers call one width-specific round function 1, 4, or 16
times. They do not expand the whole `(width, rounds)` kernel into a separate
large function. This keeps the number of round calls equal across strategies
and avoids making instruction locality or large-resource parsing an accidental
measurement axis. All NBT paths and score holders are static; there is no
runtime indexing, macro-selected element, loop counter, recursion, or branch
inside a round.

## Placement strategies

- `storage_roundtrip` uses one scratch score. Every round loads each element,
  performs the two arithmetic operations, and immediately stores it back.
- `score_cached` loads all `width` elements into distinct scores before the
  first round, performs every round on those scores, and writes all elements
  back once.
- `hot_4_cache` keeps the first `min(width, 4)` elements in distinct scores.
  Remaining elements use the storage-roundtrip path in every round; the hot
  elements are written back once at the end.

The transform therefore uses one carrier score for `storage_roundtrip`,
`width` carrier scores for `score_cached`, and up to four hot scores plus one
cold scratch score for `hot_4_cache`. Elapsed time is consequently the joint
cost of NBT traffic and score footprint, not an isolated NBT primitive
benchmark. The width matrix exposes the cost of fully scalarizing a wider
aggregate, while the hybrid represents a four-value score budget with the
remaining values spilled to storage.

This experiment covers fixed-width homogeneous integer aggregates with static
paths and a required materialization boundary. It does not cover aliases,
escaping references, dirty subsets, dynamic indexes, variable-length arrays,
heterogeneous fields, or choosing which fields are hot.

## Cost model and pack size

Let `W` be the width, `R` the round count, and `H = min(W, 4)`. The measured
Worldless command quota follows these exact formulas:

```text
storage_roundtrip = 74 + 8W + R + 6WR
score_cached      = 74 + 8W + R + 2WR + 4W
hot_4_cache       = 74 + 8W + R + 2HR + 4H + 6(W - H)R
```

The common `74 + 8W + R` term contains validation, initialization, dispatch
and round calls, checksum, and publication. The update arithmetic is `2WR`
for every strategy. A storage load or store uses one source command but two
quota units because `execute store` runs a nested command.

These formulas give three useful controls:

- all strategies have equal quota when `R = 1`;
- `score_cached` and `hot_4_cache` have equal quota when `W <= 4`;
- relative to `storage_roundtrip`, full caching saves `4W(R - 1)` quota and
  hot caching saves `4H(R - 1)` quota.

Every strategy owns 25 function resources and 228 nonblank source commands,
including its public wrapper, begin/round/end functions, and 12 drivers. Equal
source-line counts are deliberate: the begin load, repeated round body, end
write-back, and driver call skeleton sum to the same generated command count
for each strategy even though their executed quota differs.

The runtime payload (`pack.mcmeta` and `data`, excluding this README) is 69,933
bytes, with 87 function resources and 925 nonblank command lines. The longest
line is 165 bytes. The portion sizes are:

| Portion | Bytes | Functions | Nonblank commands |
| --- | ---: | ---: | ---: |
| Shared validation, initialization, checksum, and dispatch | 21,613 | 12 | 241 |
| `storage_roundtrip` | 16,634 | 25 | 228 |
| `score_cached` | 15,757 | 25 | 228 |
| `hot_4_cache` | 15,816 | 25 | 228 |

`pack.mcmeta` accounts for the remaining 113 bytes. Generated source size and
carrier-score count are separate axes because pack loading and parsing are not
part of the invocation timings.

## Measurement

From the repository root, run correctness and persistent Worldless comparison
with:

```sh
cargo run -p worldless-lab -- check --suite scalar_replacement --format text
cargo run --release -p worldless-lab -- compare --suite scalar_replacement \
  --execution persistent --warmup 1 --samples 31 --format text
```

## Results

### Worldless

The persistent command above produced these results on 2026-08-31 on an AMD
Ryzen 9 9950X3D running Linux 7.0.0-29-generic with rustc 1.98.0. Each cell is
`quota; median`, with median time in microseconds. Times are
environment-dependent, and Worldless traverses rows in fixed table order.

| Width | Rounds | Storage roundtrip `quota; median` | Score cached `quota; median` | Hot 4 cache `quota; median` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 89; 17.430 | 89; 17.300 | 89; 17.211 |
| 1 | 4 | 110; 21.100 | 98; 19.011 | 98; 18.930 |
| 1 | 16 | 194; 36.150 | 134; 26.210 | 134; 26.040 |
| 4 | 1 | 131; 26.160 | 131; 26.270 | 131; 26.170 |
| 4 | 4 | 206; 39.620 | 158; 32.100 | 158; 32.340 |
| 4 | 16 | 506; 94.251 | 266; 56.430 | 266; 56.370 |
| 8 | 1 | 187; 38.080 | 187; 37.930 | 187; 38.810 |
| 8 | 4 | 334; 67.120 | 238; 51.570 | 286; 58.190 |
| 8 | 16 | 922; 174.641 | 442; 96.831 | 682; 137.500 |
| 16 | 1 | 299; 60.400 | 299; 61.260 | 299; 61.180 |
| 16 | 4 | 590; 113.610 | 398; 87.571 | 542; 109.230 |
| 16 | 16 | 1,754; 328.401 | 794; 182.231 | 1,514; 298.771 |

All quotas matched the formulas. At one round, when no placement removes a
repeated transfer, all three quotas were equal and the medians were within 2.4
percent. At width 16 and 16 rounds, full caching used 54.7 percent less quota
and had a 44.5 percent lower median than storage roundtrip. Keeping only four
hot values used 13.7 percent less quota and had a 9.0 percent lower median.

At width 8 and 16 rounds, the four-value cache was 21.3 percent faster than
storage roundtrip, while full caching was 44.6 percent faster. Within this
bounded matrix, repeated reuse paid for both partial and full scalarization;
the full-cache advantage grew with the number of cached fields and rounds.
The short equal-quota controls also show the fixed-order noise floor, so small
differences must not be treated as a strategy ranking.

Worldless timing alone does not establish the same thresholds for Minecraft
Java Edition. Runtime implementation, batching, and row-order controls differ,
so comparisons should remain within one runtime.

### Minecraft Java Edition

The same 12 inputs and all three public entry points were checked and measured
on Minecraft Java dedicated server `26.3-snapshot-10` on 2026-08-31. The host
was the same Ryzen 9 9950X3D running Linux 7.0.0-29-generic. The server used
Microsoft OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, `--nogui`, and no players.
The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 was
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

Minecraft's stopwatch has 1 ms resolution, so a sample was an unrolled batch
of `B` consecutive public calls. Calibration selected a power-of-two `B`, up
to 1,024, toward 100 ms while keeping `B` times the Worldless quota below
800,000. The maximum-`B` rule left many short batches below the preferred
50 ms floor. Each of two fresh JVMs discarded 20 warm-up batches and measured
15 batches per row; the second JVM traversed all rows in reverse order. The
table pools the 30 unfiltered batch averages. Each cell is
`B; median / nearest-rank p95`, in microseconds per call.

Every single-call preflight required success with value 1. Both preflight and
post-batch checks required exact public output without extra fields, the
expected internal checksum, the exact final IntArray, and the expected width.
Input installation, output clearing, calibration, preflight, and all checks
were outside the stopwatch. Each timed call still included validation,
generator initialization, update rounds, write-back, checksum, and
publication.

The two measured JVMs recorded 139 and 138 young collections, no full
collections, and maximum pauses of 22.615 and 25.763 ms. Samples were not
filtered, so pauses inside timed batches remain in the results.

| Width | Rounds | Storage roundtrip `B; median / p95` | Score cached `B; median / p95` | Hot 4 cache `B; median / p95` |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 1,024; 9.766 / 10.742 | 1,024; 9.766 / 10.742 | 1,024; 9.766 / 10.742 |
| 1 | 4 | 1,024; 11.719 / 12.695 | 1,024; 9.766 / 10.742 | 1,024; 10.742 / 10.742 |
| 1 | 16 | 1,024; 20.508 / 29.297 | 1,024; 12.695 / 13.672 | 1,024; 12.695 / 13.672 |
| 4 | 1 | 1,024; 13.672 / 14.648 | 1,024; 13.672 / 14.648 | 1,024; 13.672 / 14.648 |
| 4 | 4 | 1,024; 21.484 / 22.461 | 1,024; 15.625 / 16.602 | 1,024; 15.625 / 16.602 |
| 4 | 16 | 1,024; 52.734 / 54.688 | 1,024; 24.414 / 25.391 | 1,024; 24.414 / 25.391 |
| 8 | 1 | 1,024; 19.531 / 20.508 | 1,024; 19.531 / 20.508 | 1,024; 19.531 / 20.508 |
| 8 | 4 | 1,024; 35.156 / 36.133 | 1,024; 23.438 / 24.414 | 1,024; 29.297 / 31.250 |
| 8 | 16 | 512; 95.703 / 97.656 | 1,024; 40.039 / 41.016 | 1,024; 68.359 / 70.313 |
| 16 | 1 | 1,024; 31.250 / 32.227 | 1,024; 29.297 / 30.273 | 1,024; 30.273 / 31.250 |
| 16 | 4 | 1,024; 59.570 / 61.523 | 1,024; 37.109 / 39.063 | 1,024; 54.688 / 56.641 |
| 16 | 16 | 256; 179.688 / 183.594 | 512; 68.359 / 76.172 | 512; 150.391 / 162.109 |

At width 16 and 16 rounds, full caching had a 62.0 percent lower median than
storage roundtrip; the four-value cache was 16.3 percent lower. At width 8 and
16 rounds, the corresponding reductions were 58.2 and 28.6 percent. The
one-round rows had equal quota, and their medians were equal through width 8;
the width-16 range was 6.7 percent. These short controls do not justify a
placement ranking, but the repeated-use rows consistently show the benefit of
removing NBT transfers.

Minecraft showed a larger full-cache advantage than Worldless at the widest,
highest-reuse row. The runtimes nevertheless agree only on the qualitative
result: repeated reuse favored scalarization, while a four-value score budget
retained a smaller benefit. Their absolute times are not paired latencies
because Minecraft measures server-JVM batches with a 1 ms clock and Worldless
measures individual calls with its host clock.

## Failure boundaries

Disposable Worldless checks covered each missing field, an extra field, wrong
NBT types, unsupported widths and round counts, and signed-32-bit seed
boundaries. Invalid calls returned failure without changing pre-existing work
or public output. Minimum, zero, and maximum valid IntTag seeds all produced
the independently calculated final IntArray and checksum.

Command-limit interruption is not transactional. For each variant's width-16,
16-round row, a limit equal to its successful Worldless quota produced
`CommandLimitExceeded`; one additional command succeeded. At limit 100 the
work state was partial and the old public output remained. A subsequent
full-limit call on the same VM succeeded because initialization replaced the
interrupted work state. Execution status, rather than apparently complete
storage or output, is authoritative at a quota boundary.

A separate fresh Minecraft JVM ran the width-16, 16-round storage-roundtrip row
with an empirical command-sequence limit of 100. The server reported stopping
after exactly 100 commands; the surrounding `execute store` retained its
return sentinel of -999, the expected public output and internal checksum were
absent, and the allocated width was 16 but the values were incomplete.
Restoring the normal limit of 1,000,000 and rerunning the full preflight on the
same JVM succeeded. This probe was not mixed into calibration, warm-up,
timing, or the two measured JVMs' GC results.

## Compiler implications

For a fixed aggregate that stays unaliased until a known materialization
boundary, repeated uses should be promoted to scores instead of paying an NBT
load and store around every update. A compiler cost model can count four
Worldless quota units saved per promoted value after the first round, then
trade that saving against the number of simultaneously live score holders and
the target runtime's measured score cost.

The four-value hybrid demonstrates the spill decision directly: it preserves
the benefit for a bounded hot set but continues to pay round-trip cost for cold
fields. This matrix does not identify a universal score budget or spill
threshold. Those depend on live ranges, aliases, dirty write-back, surrounding
register pressure, generated resource size, and the target runtime. The safe
optimization requires proof that no intervening operation observes or mutates
the storage slots, followed by write-back at every required observation or
escape boundary.
