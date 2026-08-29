# Worldless transformer experiment

This uv project trains the fixed candidate-B decoder on the pinned TinyStories
dataset and provides float, fake-runtime, and exact integer inference paths.
All generated artifacts are content-checked against the tokenizer and model ABI.

```bash
uv sync --locked
uv run --locked worldless-transformer train-tokenizer --output artifacts/tokenizer.json
uv run --locked python ../../packs/transformer/generate.py artifacts/tokenizer.json artifacts/datapack
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split train --output artifacts/train.bin
uv run --locked worldless-transformer preprocess --tokenizer artifacts/tokenizer.json --split validation --output artifacts/validation.bin
uv run --locked worldless-transformer train --tokenizer artifacts/tokenizer.json --train-tokens artifacts/train.bin --validation-tokens artifacts/validation.bin --output artifacts/model.pt --steps 10000 --batch-size 32 --learning-rate 0.0003 --seed 1 --device cuda --mode fake_runtime --validation-batches 32
```

`preprocess` writes a token stream, story offsets, deterministic training
windows, and a checked JSON sidecar. The exact reference can produce a
layer-by-layer golden trace for a data-pack run:

```bash
uv run --locked worldless-transformer trace --tokenizer artifacts/tokenizer.json --checkpoint artifacts/model.pt --prefix "Once upon a time" --output artifacts/trace.json
uv run --locked worldless-transformer export --tokenizer artifacts/tokenizer.json --checkpoint artifacts/model.pt --output artifacts/command_storage_transformer.dat
```

The `.dat` file does not contain its namespace. Load it as `transformer`, for
example with
`vm.load_command_storage_files([("transformer", path)])`; the default storage
path then resolves to `transformer:model`, which is what the data pack reads.
The generated data pack and model bundle must come from the same tokenizer;
inference rejects a mismatched tokenizer ID before modifying runtime state.
