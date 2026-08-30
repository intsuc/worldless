# Worldless transformer experiment

This uv project trains six explicitly named decoder-only transformer
architectures on the pinned TinyStories dataset and provides float,
fake-runtime, and exact integer inference paths. They use pre-norm layers,
ReLU-squared feed-forward layers, ALiBi, local multi-query attention, and the
same greedy StringPiece tokenizer with 510 regular pieces plus BOS and EOS.
Tokenizer and token-stream sidecars bind to the shared
`worldless_transformer/gsp512_c256_w64_v1` data ABI through the exact
`data_abi_id` field, independently of the selected model architecture.

```bash
uv sync --locked
uv run --locked worldless-transformer train-tokenizer --output artifacts/tokenizer.json
uv run --locked python ../../packs/transformer/generate.py --architecture efficient_q4_wide artifacts/tokenizer.json artifacts/datapack
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split train --output artifacts/train.bin
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split validation --output artifacts/validation.bin
uv run --locked worldless-transformer train --architecture efficient_q4_wide --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/model.pt --batch-size 32 --epochs 2 --learning-rate 0.00003 --seed 1 --device cuda --mode fake_runtime --validation-batches 256 --warmup-steps 2671
```

`preprocess` writes a token stream, story offsets, deterministic training
windows, and a checked JSON sidecar. `train` requires exactly one or two epochs.
For each epoch it visits every training window once in a new deterministic
permutation without replacement and uses a smaller final batch when necessary.
The optimizer and learning-rate schedule remain continuous across the epoch
boundary; their global step count is derived from the checked window artifact.

The training CLI requires one of these six known architectures; there is no
implicit model default:

| CLI name | Layers | Width | Query heads | FFN width | Value-embedding blocks | Parameters | Runtime denominator |
| --- | ---: | ---: | --- | ---: | --- | ---: | ---: |
| `baseline` | 4 | 96 | 6×16 | 192 | none | 282,624 | 16 |
| `efficient` | 4 | 96 | 6×16 | 96 | `1,3` (512×16) | 274,432 | 16 |
| `efficient_q4` | 4 | 96 | 4×24 | 96 | `1,3` (512×24) | 288,768 | 24 |
| `efficient_q4_ff192` | 4 | 96 | 4×24 | 192 | `1,3` (512×24) | 362,496 | 24 |
| `efficient_q4_wide` | 4 | 128 | 4×32 | 128 | `1,3` (512×32) | 458,752 | 32 |
| `efficient_q4_deep` | 8 | 96 | 4×24 | 96 | `1,3,5,7` (512×24) | 479,232 | 24 |

`blocks.1` and `blocks.3` are zero-based: the value tables are used in the
second and fourth transformer blocks. `blocks.5` and `blocks.7` extend the
same alternating pattern in the eight-layer model. The efficient architectures
add each int8 value-projection row and token-table row in int32, clamp the
result to `[-127, 127]`, and only then supply it to attention. The q4 models
use the first four ALiBi slopes. Every model except `baseline` has an
independent output head; `baseline` ties it to the token embedding. Their
float, fake-runtime, and exact-reference paths share the same architecture
contract. A checkpoint records the exact architecture ID and loads only
through that known specification.

Training ablations are explicit. The checkpoint's required six-field schema
stores the attention denominator needed by inference. This deliberately
non-deployable example combines a training-only logit softcap, attention scale
denominator 11, and a 40-step warmup plus 65% linear warmdown:

```bash
uv run --locked worldless-transformer train --architecture baseline --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/ablation-d11.pt --batch-size 32 --epochs 1 --learning-rate 0.00003 --seed 1 --device cuda --mode fake_runtime --validation-batches 32 --logit-softcap 15 --attention-logit-denominator 11 --warmup-steps 40 --warmdown-ratio 0.65 --final-learning-rate-fraction 0.05 --learning-rate-decay linear
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

All six architectures can be exported, traced, and generated when their
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
no checkpoint. The deployable q6 and q4 runs therefore used the previously
validated full-horizon schedule: peak `3e-5`, 2% warmup, then cosine decay to
zero.

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
`efficient`, so `efficient_q4` became the control for the scaling experiment.
EOS loss and EOS accuracy are reported separately because EOS is not part of
the BPB denominator.

The training-only softcap was rejected using uncapped raw logits, which are
what greedy deployment observes. On the fixed pilot evaluation, denominator
16 without a softcap scored 2.91283 BPB. A softcap of 15 appeared better under
its capped objective (2.85121 BPB) but scored 3.14869 BPB when reevaluated with
raw logits; combining it with denominator 11 scored 3.19683 raw-logit BPB.
The q4 model instead uses denominator 24 as part of its fixed architecture
contract. No runtime softcap or alternate-denominator fallback exists.

## Scaling experiment

The size sweep first isolated three directions from the selected q4 model:
doubling only the FFN, widening the residual stream, and doubling only the
depth. Each pilot used the same 50,000-story subset, 99,993 windows, 17,430,919
supervised targets, seed-1 permutation, batch size 32, and one epoch. The
optimizer used peak LR `1e-3`, 40 warm-up steps, and a 65% linear warmdown.
The table reports all 43,222 validation windows rather than the sampled metric
saved during training.

| Pilot | Parameters | Dense + W64 attention MACs/position | Attention denominator | Full BPB |
| --- | ---: | ---: | ---: | ---: |
| q4 control | 288,768 | 264,192 | 24 | 2.717126 |
| FFN 192 | 362,496 | 337,920 | 24 | 2.743404 |
| Width 128 | 458,752 | 425,984 | 24 | 2.659402 |
| Width 128 | 458,752 | 425,984 | 32 | **2.612061** |
| Eight layers | 479,232 | 479,232 | 24 | 2.777671 |

The FFN-only and depth-only directions were slower and worse than the control,
so they were rejected. A second seed confirmed the deployable wide model:
control BPB was 2.720774 and wide/denominator-32 BPB was 2.660426. Across the
two seeds, widening improved mean pilot BPB by 3.04%. Denominator 24 did not
replicate that gain and is intentionally non-deployable for the wide ABI.

The winner was then trained from scratch over the full corpus. The one-epoch
run used every window once. Its loss was still improving at the end, so a
fresh two-epoch follow-up was trained: each epoch has its own
deterministic permutation, while AdamW and a single global cosine schedule
continue across the boundary. Its 2,671-step warm-up matches the one-epoch
run's absolute warm-up length.

| Model | Epochs | Window visits | Optimizer steps | Loss | Perplexity | BPB | EOS loss | EOS accuracy |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| q4 control | 1 | 4,272,918 | 133,529 | 4.877349 | 131.282 | 2.744239 | 3.613643 | 5.875% |
| Width 128 | 1 | 4,272,918 | 133,529 | 4.703760 | 110.361 | 2.647037 | 3.201337 | 8.308% |
| Width 128 | 2 | 8,545,836 | 267,058 | **4.155334** | **63.773** | **2.337949** | **3.107815** | 2.187% |

Widening adds 58.87% parameters and 61.24% dense-plus-full-window MACs. At one
epoch it improves BPB by 3.54% over q4. The second epoch improves BPB by a
further 11.68%, or 14.81% relative to q4. EOS loss improves, but EOS argmax
accuracy falls in the two-epoch run, so greedy generation tends not to stop on
its own.

Eight fixed prompts with 64 greedy continuation tokens expose a second quality
tradeoff:

| Model | Mean distinct-2 | Mean distinct-3 | Longest identical-token run | Prompts reaching EOS |
| --- | ---: | ---: | ---: | ---: |
| q4 control | 0.617 | 0.745 | 64 | 1 / 8 |
| Width 128, 1 epoch | **0.760** | **0.869** | 8 | 0 / 8 |
| Width 128, 2 epochs | 0.591 | 0.679 | **4** | 0 / 8 |

The wide model produces more locally grammatical text and fewer malformed
fragments than q4, but the two-epoch greedy output often repeats whole clauses
such as `She was very happy.` Its much better held-out BPB therefore does not
make it a production-quality storyteller. The one-epoch artifact remains the
better diversity reference; the two-epoch artifact is the held-out-loss winner
and the basis of the final runtime measurements below.

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

The current q4 model and the two-epoch wide model were measured twice in
counterbalanced order. Every run discarded 20 warm-up calls and immediately
measured 30 calls; the table pools the 60 measured samples. p95 uses the
nearest-rank definition. Worldless is commit
`448737790f33633b5df327c6f718d13b0867a043`, which includes cached string and
identifier hashes, a single-result NBT-path fast path, and allocation-free
number-provider iteration.

| Runtime | Architecture | Entry | Samples | Median | p95 | Min–max | Inference quota |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| Worldless | q4 control | `infer/text` | 60 | 226.327 ms | 226.738 ms | 225.600–227.664 ms | 29,962 |
| Worldless | Width 128, 2 epochs | `infer/text` | 60 | 378.314 ms | 380.139 ms | 376.981–380.253 ms | 37,462 |
| Minecraft | q4 control | `infer/text` | 60 | 208 ms | 219 ms | 196–220 ms | — |
| Minecraft | Width 128, 2 epochs | `infer/text` | 60 | 355 ms | 375 ms | 350–382 ms | — |

For this two-position request, widening increases the Worldless median by
67.15%, Minecraft median by 70.67%, and command quota by 25.03%. The model-size
gain is therefore a deliberate quality/latency tradeoff, not a free scaling
win. The recent Worldless VM optimization independently reduced the same q4
median from the earlier 317.126 ms to 226.327 ms (28.63%) without changing its
29,962-command quota. These short-prefix results must not be extrapolated to a
64-key attention window without a separate measurement.

Rerun the selected two-epoch wide Worldless measurement from the repository
root with:

```bash
cargo run --release -p worldless-lab -- benchmark \
  --pack crates/worldless-lab/experiments/transformer/artifacts/scaling_v1/full/wide_ad32_2epoch/pack \
  --model-storage crates/worldless-lab/experiments/transformer/artifacts/scaling_v1/full/wide_ad32_2epoch/model.dat \
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
1,374/1,377 ms for q4 and 2,291/2,306 ms for wide. The final ten warm-ups had
medians of 198/218.5 ms for q4 and 357/353.5 ms for wide. The measured phases
had 18 young-GC pauses across the q4 runs and 36 across the wide runs; the
maximum pause was 1.620 ms. No sample was filtered and no full GC occurred.
The two q4 JVMs formed distinct but internally stable 198 ms and 218 ms regimes,
so the pooled 208 ms median is the mean of the two central samples rather than
a value observed inside either run. The two wide runs both had 355 ms medians.

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

| Artifact | q4 control | Width 128, 1 epoch | Width 128, 2 epochs |
| --- | --- | --- | --- |
| Checkpoint | 1,173,383 bytes; `85f7959fe261567fc42cfe5e106f73a52cc82acaecba34a1172a03e640f1409b` | 1,853,319 bytes; `82d89b683ac94a2d5d0a614c2132f9bfaec3b0fc1d006e7e65a796094597bf6a` | 1,853,319 bytes; `d5e56e04728062c5a9179509029c362efe5aaf6cdbc4717c3651fbd610b657d0` |
| Run manifest | schema 3; `c760f7279132b66246d09dd38ebf5981e2b879bbcc6760e4334c85dfe03856f4` | schema 3; `32f6ab3babccb8edde43503d4c846eaa768c91658efb9fb06d9d926a77abbf1e` | schema 3; `10cda33d70583adddf0b6cbacb4e34c42242adc495806a386c944645a5ea8f4e` |
| `model.dat` | 143,704 bytes; `597062241ba99f66e0fdd0e53e906af9ddbe3b93bfb98600a4fbf458c736df5c` | 222,632 bytes; `96fadc5815cf6c1c71a037b05171012abb7d3598dd5cb73c85779ef032e1512b` | 209,176 bytes; `dbf9376c036ec76f6a7696b3c4a7d82acac0582c08a29b76ce663cd8f3d7f505` |
| Generated pack | 36,307,936 bytes / 1,529 files; tree `70628fd63c83127471214f87a048ab008a2139cdf9e61638a242f487fa6f02ca` | 50,303,097 bytes / 1,528 files; tree `0be19740443241cafbf1e46a32d6217adcb922182e00a6d2ff6c7d239b51a031` | same byte-for-byte pack |
| `Once` trace | `b6c83070535ed8c2126f3d056f26643a10367b32f29c50968ec3541a6fdfde3a` | `5013dddc3fffbb369d83d8eaa4f8ff58343d351a4534844e50357ca17d7d119d` | `3bfd49f6876a5e048e4e6e5838cafad6bd13e4267fbfadb12388fd793ed40dbf` |

The scaling results are stored in `artifacts/scaling_v1/full`:

- `quality-final.json`: `2febb2c78365ae8910e5b334d8efa81dbed4bf5c8dbd00c5d4fff9ee75639a8b`
- `worldless-benchmark-final.json`: `5f7436897e02d9d89ebd8fe950279e845c5ed2c5373e4bf98a9c17de9e801f36`
- `minecraft-benchmark-final.json`: `5e70e620e24887700d567adaeaba9bd2516503f741edcbaf9f16f2170771c612`

For the one- and two-epoch wide models, Python and Worldless agree exactly at
every after-attention and after-FFN layer output, final hidden state, all 512
logits, and the greedy token. The official exporter and pack generator also
reproduce the stored `.dat` and pack byte-for-byte.

This is a warm, repeated-prefix benchmark, not a cold-start or novel-prefix
benchmark. Its two-position request never approaches the local attention
window of 64 or the context limit of 256. Measure the concrete production
model at full window before using these results for capacity planning.
