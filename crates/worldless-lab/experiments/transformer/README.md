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
