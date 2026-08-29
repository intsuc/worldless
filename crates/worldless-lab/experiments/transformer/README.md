# Worldless transformer experiment

This uv project trains a fixed decoder-only transformer on the pinned
TinyStories dataset and provides float, fake-runtime, and exact integer
inference paths. The model uses four pre-norm layers, width 96, ReLU-squared
feed-forward layers, ALiBi, local multi-query attention, and a greedy
StringPiece tokenizer with 510 regular pieces and a 512-token vocabulary that
includes BOS and EOS. All generated artifacts are content-checked against the
tokenizer and model ABI.

```bash
uv sync --locked
uv run --locked worldless-transformer train-tokenizer --output artifacts/tokenizer.json
uv run --locked python ../../packs/transformer/generate.py artifacts/tokenizer.json artifacts/datapack
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split train --output artifacts/train.bin
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split validation --output artifacts/validation.bin
uv run --locked worldless-transformer train --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/model.pt --batch-size 32 --learning-rate 0.00003 --seed 1 --device cuda --mode fake_runtime --validation-batches 32
```

`preprocess` writes a token stream, story offsets, deterministic training
windows, and a checked JSON sidecar. `train` performs exactly one epoch: it
visits every training window once in a seeded permutation without replacement,
uses a smaller final batch when necessary, and derives the optimizer-step count
from the checked window artifact. The exact reference can produce a
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

## Runtime benchmark

The following benchmark was measured on 2026-08-29 with the one-epoch model
exported above. Both public entry points receive the same semantic input:
`"Once"` is regular piece 349, `max_new_tokens` is 1, and the implicit BOS
makes each request evaluate two model positions. Every invocation returned
token 367, the piece `" upon"`.

| Runtime | Entry point | Discarded warm-up | Measured samples | Median | p95 | Min–max | Worldless quota |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Worldless | `infer/text` | 20 | 30 | 529.53 ms | 539.33 ms | 527.32–541.00 ms | 44,907 |
| Worldless | `infer/tokens` | 20 | 30 | 530.19 ms | 536.92 ms | 527.55–539.53 ms | 44,789 |
| Minecraft | `infer/text` | 20 | 30 | 358.5 ms | 363 ms | 345–364 ms | — |
| Minecraft | `infer/tokens` | 20 | 30 | 362.0 ms | 370 ms | 354–373 ms | — |

For this short warm-state request, Minecraft's median latency was 32.3% lower
for text and 31.7% lower for tokens; the corresponding
Worldless-to-Minecraft latency ratios are 1.48 and 1.46. The small difference
between the text and token entries is within the observed run-to-run variation
and is not evidence of a tokenizer performance difference.

Each runtime used one persistent instance. Data-pack compilation or server
startup, model loading, `transformer:setup`, request writes, and response
verification were outside the timer. The timed boundary was the complete
public `transformer:infer/text` or `transformer:infer/tokens` call, including
artifact and request validation, text tokenization where applicable, KV-cache
initialization, both forward positions, and response writes. Worldless used a
release build and `Instant`; Minecraft used its built-in stopwatch, backed by
`Util.getMillis()`, at 1 ms resolution. Both command limits were 1,000,000. p95
is the nearest-rank percentile.

Minecraft ran both entries in one server JVM, in table order. Twenty identical
invocations of each entry were discarded before collecting its samples. The
first text warm-up took 475 ms and the final five had a 362 ms median; the
token-entry warm-ups started after the shared model core was hot and ended with
a 359 ms median over the final five. Neither final ten-sample warm-up window
had a downward trend. The measured text and token phases each included 17
young-GC pauses (maximum 1.738 ms and 1.177 ms respectively), with no samples
filtered and no full GC.

The host was an AMD Ryzen 9 9950X3D with 32 logical CPUs, 182 GiB RAM, and
Linux 7.0.0-29-generic; CPU affinity was not pinned, and the Worldless and
Minecraft measurements ran sequentially. Worldless was commit
`b029d5051d2cabebe97163483bb5a47fe75a1ac2`, compiled in release mode by
rustc 1.98.0. Minecraft was `26.3-snapshot-10`; its server and Java runtime were
downloaded and integrity-checked by:

```bash
cargo run -q -p worldless-dev -- generate-target
```

The server used the Mojang launcher component `java-runtime-epsilon`, reporting
Microsoft OpenJDK 25.0.1+8-LTS. Its JVM arguments were `-Xms2G -Xmx2G` and
`-Xlog:gc*=info:file=gc.log:time,uptime,level,tags`; the server used `--nogui`.
It had no players and used
`minecraft:max_command_sequence_length=1000000`. The server JAR SHA-256 was
`cdbbda7cc47e026e57be8e10d9ef097ad16f1086b63b88a469a4c0e6e4f77dbe`.
The checkpoint SHA-256 was
`53e34349dcc317a0d4fb110c305bcbe0edf99518af9d873a8127cf50a9e6e697`, the
command-storage export SHA-256 was
`a4e5e6d2082c0473430bd545ff6bfb9b75aad5533c027d44ab94a9caf9fbfdca`, and
the tokenizer ID was
`28c004b0ed25ab48e204a46e23fe74787a10f0b1ab23b97fa8ee9f6d71b64e3c`.

This is a warm, repeated-prefix public-request benchmark, not a cold-start or
novel-prefix benchmark. Its two-position request never approaches the local
attention window of 64 or the context limit of 256, so these measurements must
not be extrapolated to long-prefix or full-window inference.
