# Worldless transformer experiment

This uv project trains three explicitly named decoder-only transformer
architectures on the pinned TinyStories dataset and provides float,
fake-runtime, and exact integer inference paths. All three use four pre-norm
layers, width 96, ReLU-squared feed-forward layers, ALiBi, local multi-query
attention, and the same greedy StringPiece tokenizer with 510 regular pieces
plus BOS and EOS. Tokenizer and token-stream sidecars bind to the shared
`worldless_transformer/gsp512_c256_w64_v1` data ABI through the exact
`data_abi_id` field, independently of the selected model architecture.

```bash
uv sync --locked
uv run --locked worldless-transformer train-tokenizer --output artifacts/tokenizer.json
uv run --locked python ../../packs/transformer/generate.py --architecture efficient_q4 artifacts/tokenizer.json artifacts/datapack
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split train --output artifacts/train.bin
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split validation --output artifacts/validation.bin
uv run --locked worldless-transformer train --architecture efficient_q4 --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/model.pt --batch-size 32 --learning-rate 0.00003 --seed 1 --device cuda --mode fake_runtime --validation-batches 256
```

`preprocess` writes a token stream, story offsets, deterministic training
windows, and a checked JSON sidecar. `train` performs exactly one epoch: it
visits every training window once in a seeded permutation without replacement,
uses a smaller final batch when necessary, and derives the optimizer-step count
from the checked window artifact.

The training CLI requires one of these three known architectures; there is no
implicit model default:

| CLI name | FFN width | Output head | Value embeddings | Parameters | Runtime attention denominator |
| --- | ---: | --- | --- | ---: | ---: |
| `baseline` | 192 | tied to token embedding | none | 282,624 | 16 |
| `efficient` | 96 | independent | ungated 512×16 tables before attention in `blocks.1` and `blocks.3` | 274,432 | 16 |
| `efficient_q4` | 96 | independent | ungated 512×24 tables before attention in `blocks.1` and `blocks.3` | 288,768 | 24 |

`blocks.1` and `blocks.3` are zero-based: the value tables are used in the
second and fourth transformer blocks. The efficient architectures add each
int8 value-projection row and token-table
row in int32, clamp the result to `[-127, 127]`, and only then supply it to
attention. `efficient` uses six 16-wide query heads; `efficient_q4` uses four
24-wide query heads and the corresponding first four ALiBi slopes. Their float,
fake-runtime, and exact-reference paths share the same architecture contract.
Train one by changing the command above to `--architecture efficient` or
`--architecture efficient_q4`; its checkpoint records the exact architecture
ID and loads only through that known specification.

Training ablations are explicit. The checkpoint's required six-field schema
stores the attention denominator needed by inference. This deliberately
non-deployable example combines a training-only logit softcap, attention scale
denominator 11, and a 40-step warmup plus 65% linear warmdown:

```bash
uv run --locked worldless-transformer train --architecture baseline --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/ablation-d11.pt --batch-size 32 --learning-rate 0.00003 --seed 1 --device cuda --mode fake_runtime --validation-batches 32 --logit-softcap 15 --attention-logit-denominator 11 --warmup-steps 40 --warmdown-ratio 0.65 --final-learning-rate-fraction 0.05 --learning-rate-decay linear
uv run --locked worldless-transformer evaluate-run --tokenizer artifacts/tokenizer.json --validation-tokens artifacts/validation.bin --checkpoint artifacts/ablation-d11.pt --batch-size 32 --batches 32 --seed 1 --device cuda --mode fake_runtime
uv run --locked worldless-transformer evaluate-all-run --tokenizer artifacts/tokenizer.json --validation-tokens artifacts/validation.bin --checkpoint artifacts/ablation-d11.pt --batch-size 32 --device cuda --mode fake_runtime
```

The adjacent `<checkpoint>.run.json` owns training and optimizer provenance;
the checkpoint owns the runtime attention denominator, and strict run loading
requires both values to match. `evaluate-run` requires the exact manifest
schema and verifies that it is bound to the checkpoint and selected validation
stream by tokenizer ID, optimizer step, and SHA-256. Omitting the ablation flags
retains the architecture's fixed denominator from the table, no softcap, the
existing 2% warmup and full cosine decay to zero, and the existing AdamW
settings. `--warmup-ratio` and `--warmup-steps` are mutually exclusive. The
available attention denominators are 8, 11, 16, 24, and 32.

`evaluate-all-run` applies the same strict run-manifest checks and metrics, but
walks validation windows once in stored order without sampling. It includes the
last partial batch and reports both the evaluated window and batch counts.

All three architectures can be exported, traced, and generated when their
checkpoint uses the fixed runtime denominator in the table above. A training
ablation with another denominator remains available to `evaluate-run` and
`evaluate-all-run`, but export, trace, and generation reject it. The logit
softcap is applied only to cross-entropy during training and validation.
Fake-runtime and exact inference continue to select tokens from uncapped raw
logits.

The exact reference can produce a
layer-by-layer golden trace for a data-pack run:

```bash
uv run --locked worldless-transformer trace --tokenizer artifacts/tokenizer.json --checkpoint artifacts/model.pt --prefix "Once upon a time" --output artifacts/trace.json
uv run --locked worldless-transformer export --tokenizer artifacts/tokenizer.json --checkpoint artifacts/model.pt --output artifacts/command_storage_transformer.dat
```

The generated data pack contains the integer runtime and compiled tokenizer,
but no model weights. Export a new `.dat` after each training run. Regenerate
the data pack only when its tokenizer or runtime changes. The generator and
artifact writers refuse to replace existing outputs, so publish through new
paths before replacing deployed artifacts.

The `.dat` file does not contain its namespace. Load it as `transformer`, for
example with
`vm.load_command_storage_files([("transformer", path)])`; the default storage
path then resolves to `transformer:model`, which is what the data pack reads.
The generated data pack and model bundle must come from the same tokenizer;
inference rejects a mismatched tokenizer ID before modifying runtime state.

## Optimization experiment

The optimization run used TinyStories revision
`f54c09fd23315a6f9c86f9dc80f725de7d8f9c64`, 2,119,719 training stories,
4,272,918 non-overlapping-supervision windows, and 745,213,579 next-token
targets. Batch size 32 produced 133,528 full batches and one final batch of 22,
for 133,529 optimizer steps. Every window and target was consumed exactly once
in the seed-1 permutation.

Short 50,000-story pilots favored a 40-step warmup, long linear warmdown, and a
peak learning rate near `1e-3`. That result did not transfer to the full
training horizon: baseline runs at `1e-3`, `3e-4`, and `1e-4` developed
sustained loss increases and were stopped at steps 7,000, 15,000, and 42,000.
A `7.5e-5` run that decayed across almost the whole epoch remained stable
longer but showed the same pattern by step 75,000. These stopped runs produced
no checkpoint. The deployable q6 and q4 runs therefore use the previously
validated full-horizon schedule shown in the quick-start command: peak
`3e-5`, 2% warmup, then cosine decay to zero.

The table below evaluates every one of the 43,222 validation windows in stored
order. BPB is negative log-likelihood divided by the number of original UTF-8
bytes and is comparable across tokenizer choices. The baseline checkpoint is
the earlier strict one-epoch run; its 510-piece vocabulary is byte-for-byte
identical to the schema-2 tokenizer used by the optimized runs.

| Architecture | Parameters | Dense + W64 attention MACs/position | Loss | Perplexity | BPB | EOS loss | EOS accuracy |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `baseline` | 282,624 | 331,776 | 5.202863 | 181.792 | 2.928212 | 3.356273 | 2.024% |
| `efficient` | 274,432 | 258,048 | 4.933937 | 138.925 | 2.776183 | 3.591923 | 1.260% |
| `efficient_q4` | 288,768 | 264,192 | 4.877349 | 131.282 | 2.744239 | 3.613643 | 5.875% |

`efficient` reduces BPB by 5.19% and arithmetic by 22.22% relative to the
baseline. `efficient_q4` reduces BPB by 6.28% and arithmetic by 20.37%; it also
reduces the query-head loops from six to four. Its BPB is 1.15% below
`efficient`, so `efficient_q4` is the selected deployment architecture. EOS
loss and EOS accuracy are reported separately because EOS is not part of the
BPB denominator.

The training-only softcap was rejected using uncapped raw logits, which are
what greedy deployment observes. On the fixed pilot evaluation, denominator
16 without a softcap scored 2.91283 BPB. A softcap of 15 appeared better under
its capped objective (2.85121 BPB) but scored 3.14869 BPB when reevaluated with
raw logits; combining it with denominator 11 scored 3.19683 raw-logit BPB.
The q4 model instead uses denominator 24 as part of its fixed architecture
contract. No runtime softcap or alternate-denominator fallback exists.

The data-pack optimization was measured separately from the architecture
change. Moving full model validation and weight staging to the explicit
`transformer:model/activate` boundary, fusing each projection and its requant,
and materializing every ReLU-squared activation once reduced the two-position
token request from 44,789 to 32,896 commands and its Worldless median by about
23%. The same pack-only change reduced the real-Minecraft median from 362 ms to
289.5 ms. A single fused 512-row argmax lowered quota further but did not
improve Minecraft median and worsened p95; a chunked version was slower in
Worldless. Both argmax variants were rejected, leaving the generated row-based
argmax in the deployed pack.

## Runtime benchmark

The final benchmark was measured on 2026-08-30. `"Once"` is regular piece 349,
`max_new_tokens` is 1, and the implicit BOS makes each request evaluate two
model positions. Every Worldless and Minecraft invocation returned token 367,
the piece `" upon"`.

Each architecture was measured twice in counterbalanced order: q6 then q4,
followed by q4 then q6. Every run discarded 20 warm-up calls and immediately
measured 30 calls; the table pools the 60 measured samples. p95 uses the
nearest-rank definition.

| Runtime | Architecture | Entry | Samples | Median | p95 | Min–max | Inference quota |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Worldless | `efficient` q6 | `infer/text` | 60 | 307.890 ms | 311.048 ms | 302.890–329.540 ms | 30,726 |
| Worldless | `efficient_q4` q4 | `infer/text` | 60 | 317.126 ms | 326.989 ms | 313.720–339.474 ms | 29,962 |
| Minecraft | `efficient` q6 | `infer/text` | 60 | 209 ms | 210 ms | 207–213 ms | — |
| Minecraft | `efficient_q4` q4 | `infer/text` | 60 | 213 ms | 215 ms | 211–217 ms | — |

For this two-position request, q4 uses 2.49% fewer commands than q6 but has a
3.00% higher Worldless median and a 1.91% higher Minecraft median. The q4
choice is therefore not a claim that fewer heads always reduce short-request
latency. It is selected for its better full-validation quality and long-context
cost: the zero-weight 64-token capacity fixture uses 2,116,606 commands for q4,
versus 2,746,366 for q6 and 2,816,894 for the baseline. The previous
pre-optimization text benchmark measured 529.53 ms in Worldless and 358.5 ms
in Minecraft, so the final q4 medians are respectively 40.1% and 40.6% lower
on the same host. These historical comparisons include both architecture and
data-pack changes.

Rerun the selected q4 Worldless measurement from the repository root with:

```bash
cargo run --release -p worldless-lab -- benchmark \
  --pack crates/worldless-lab/experiments/transformer/artifacts/optimization_v2/full/efficient_q4/pack \
  --model-storage crates/worldless-lab/experiments/transformer/artifacts/optimization_v2/full/efficient_q4/model.dat \
  --entry text --request '{prefix:"Once",max_new_tokens:1}' \
  --warmup 20 --samples 30 --quota 100000 --format json
```

Each Worldless run used a fresh VM that remained persistent across its 50
calls; each Minecraft run used a fresh server JVM. Pack compilation or server
startup, command-storage installation, `transformer:setup`, full model
validation and staging in `transformer:model/activate`, request writes, and
response checks were outside the timer. The timed boundary was the complete
public `transformer:infer/text` call. It includes the active-snapshot check,
request validation, text tokenization, KV-cache initialization, both forward
positions, and response writes; it does not repeat full artifact validation.
Worldless used a release build and `Instant`. Minecraft used its built-in
stopwatch at integer-millisecond resolution.

JVM warmup was handled inside each clean server. The first invocation took
1,319–1,440 ms across the four runs. The final ten warm-ups had medians of
209/210 ms for q6 and 213.5/213.5 ms for q4, with slopes between -0.133 and
+0.218 ms per invocation. The measured phases had eight young-GC pauses per q6
run and nine per q4 run; the maximum pause was 1.544 ms. No sample was filtered
and no full GC occurred.

The host was an unpinned AMD Ryzen 9 9950X3D with 32 logical CPUs, 182 GiB RAM,
and Linux 7.0.0-29-generic. Worldless and Minecraft measurements ran
sequentially without another CPU benchmark. Minecraft `26.3-snapshot-10` and
its Java runtime were downloaded and integrity-checked by:

```bash
cargo run -q -p worldless-dev -- generate-target
```

The server used Mojang launcher component `java-runtime-epsilon`, Microsoft
OpenJDK 25.0.1+8-LTS, `-Xms2G -Xmx2G`, and
`-Xlog:gc*=info:file=gc.log:time,uptime,level,tags`; it ran `--nogui` with no
players and `minecraft:max_command_sequence_length=1000000`. The server JAR
SHA-256 is
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`,
and the Java executable SHA-256 is
`75ca070bd7f7e3ae53441509f25483951ec20a0ef42aef6a2dc8ea02531b3952`.

## Final artifacts

Both production packs use tokenizer ID
`45afd1bbddf6fbe6e11b0b0540ae93471f7100a7f654befb2b7358038685892f`;
the tokenizer JSON SHA-256 is
`413a5e765b2ef71c4034a6b0400c3ed251a52e73d3e86a682346a710e4786d49`.

| Artifact | `efficient` q6 | `efficient_q4` q4 |
| --- | --- | --- |
| Checkpoint SHA-256 | `0073cedf2fde2e65505a0e3d29ac4ff14ebfe8f0d379751067860f873e74c289` | `85f7959fe261567fc42cfe5e106f73a52cc82acaecba34a1172a03e640f1409b` |
| `model.dat` | 136,515 bytes; `5a092b77eab46ae79e43a5b37cbb441a590a363dfd84de16c4c7428bbf2feef7` | 143,704 bytes; `597062241ba99f66e0fdd0e53e906af9ddbe3b93bfb98600a4fbf458c736df5c` |
| Generated pack | 34,303,758 bytes / 1,657 files; tree `936a3f2afa9ab00b322192deae85fd69fd9c527b199fc1e321cc8fcda4ebd0e1` | 36,307,936 bytes / 1,529 files; tree `70628fd63c83127471214f87a048ab008a2139cdf9e61638a242f487fa6f02ca` |

The artifact index is
`artifacts/optimization_v2/full/artifacts.json` (SHA-256
`6935697aa62d9fbd17944959ed912b5f053ff8cdd72383ac1d8b9525ef8933d1`).
The raw counterbalanced results are
`worldless-benchmark-text-once.json` (`cdfd66f7...a0920`) and
`minecraft-benchmark-once.json` (`b300fc53...30720`) in the same directory.
For both models, Python and Worldless agree exactly at every after-attention and
after-FFN layer output, final hidden state, all 512 logits, and the greedy token.

This is a warm, repeated-prefix benchmark, not a cold-start or novel-prefix
benchmark. Its two-position request never approaches the local attention
window of 64 or the context limit of 256. Measure the concrete production
model at full window for capacity planning; the zero-weight fixture above is a
structural comparison, not an exact production quota.
