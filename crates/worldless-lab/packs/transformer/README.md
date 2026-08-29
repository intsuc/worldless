# Integer decoder transformer pack

This data pack executes a decoder-only transformer with integer operations in
Worldless and Minecraft. The generator supports exactly three runtime
architectures. All use four pre-norm layers, width 96, one KV head,
ReLU-squared FFNs, ALiBi, context 256, and local attention window 64:

- `worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1`
  uses FFN width 192 and a tied 512-token embedding/unembedding.
- `worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff96_untied_ve13_c256_w64_v1`
  uses FFN width 96, an untied language-model head, and token value embeddings
  in zero-based `blocks.1` and `blocks.3` (the second and fourth blocks).
- `worldless_transformer/relu2_alibi_gsp512_l4_d96_q4_kv1_h24_ff96_untied_ve13_ad24_c256_w64_v1`
  uses four width-24 query heads, a width-24 KV head, FFN width 96, the untied
  head and value embeddings, and attention-logit denominator 24.

The checked-in fixture pack uses the `baseline` architecture. Production pack
generation requires an explicit architecture selection.

## Runtime contract

Run `transformer:setup` once after the pack is loaded. It owns the scoreboard
objective and the runtime RMS/softmax tables; it does not install or activate
model weights.

Install one model compound at command storage `transformer:model`. Its only
top-level fields are `abi`, `weights`, `biases`, and `shifts`.
`weights` contains the exact flattened matrices returned by
`worldless_transformer.spec.expected_weight_shapes(spec)` for the selected
`spec` as `ByteArrayTag` values in -127..127. Every corresponding `shifts`
value is an
`IntArrayTag[1]`; `biases` is an empty `CompoundTag`; and
`abi.tokenizer_id` is an `IntArrayTag[8]`. The exporter is the canonical
owner of these NBT types. Runtime validation additionally rejects unknown or
missing fields, wrong shapes, -128 weights, and shifts outside 0..30. Token
embedding, untied language-model head, and value-embedding shifts must be zero.

After writing the model, call `transformer:model/activate`. Activation validates
the complete source compound, stages it into an internal inactive bank, then
publishes that bank and returns 1 in one final commit command. A validation
failure or command-limit interruption before that command leaves the previous
snapshot intact. Once the commit command executes, the new snapshot is active,
even if the caller also observes a command-limit interruption while the command
queue drains. If no activation has reached the commit, inference fails with
error 1. Changing `transformer:model` has no effect until an activation reaches
the commit. `transformer:a0`, `transformer:a1`, `transformer:runtime`, and the
pack's scoreboard entries are internal state and must not be modified by callers.
The checked-in baseline fixture's fresh activation reports `quota_used = 5,111`;
reactivation from bank 0 reports 5,112. In Worldless, limit 5,111 interrupts
before that reactivation's commit, limit 5,112 executes the commit and then
reports `CommandLimitExceeded` while draining the queue, and limit 5,113
completes with success 1. The `efficient` pack reports 5,054 for fresh
activation and 5,055 for reactivation; its corresponding pre-commit,
post-commit, and clean reactivation limits are 5,054, 5,055, and 5,056.
Configure at least the clean limit for the selected architecture. The
`efficient_q4` pack reports 5,276 for fresh activation and 5,277 for
reactivation; its corresponding limits are 5,276, 5,277, and 5,278. A data pack
cannot inspect the caller's remaining command quota.

The public entry points read exact request compounds from
`transformer:request`:

- `transformer:infer/text` accepts only
  `{prefix:<StringTag>,max_new_tokens:<IntTag>}`.
- `transformer:infer/tokens` accepts only
  `{prefix_tokens:<IntArrayTag>,max_new_tokens:<IntTag>,tokenizer_id:<IntArrayTag[8]>}`.
  The tokenizer ID must equal the model ABI, and every prefix token must be a
  regular piece in 0..509. BOS 510 is prepended internally; BOS and EOS 511 are
  not valid caller-supplied prefix tokens.

Unknown, missing, or wrongly typed request fields fail before inference state or
the KV cache is committed. `max_new_tokens` is in 1..256, and the tokenized
prefix plus requested generation must satisfy the fixed 256-position model
input context. Success writes
`{ok:1b,generated:<IntArrayTag>,final_hidden:<IntArrayTag[96]>}` to
`transformer:response`. Failure writes `{ok:0b,error:<IntTag>}` and returns
failure. Error values are:

1. no activated model snapshot;
2. text pack tokenizer does not match the model;
3. invalid request, including a token-request tokenizer mismatch;
4. unsupported text scalar;
5. prefix token outside 0..509;
6. requested model-input context exceeds 256.

The text entry point is a generated greedy-longest-match StringPiece trie.
Prefix contents are read only as StringTag data and are never inserted into a
macro. Trie transitions preserve Unicode scalar boundaries while advancing
Minecraft's Java UTF-16 indices by one or two code units. The tokenizer
artifact requires a single-scalar base piece for every scalar used by any
longer piece, so supported text is lossless; a scalar absent from the artifact
fails with error 4.

## Generate a tokenizer-specific pack

Compile a trained TinyStories tokenizer artifact into a new, non-existing
output directory with the locked uv project:

```sh
uv run --locked --project crates/worldless-lab/experiments/transformer \
  python crates/worldless-lab/packs/transformer/generate.py \
  --architecture baseline TOKENIZER_JSON OUTPUT_DIR
```

`--architecture` is required and accepts `baseline`, `efficient`, or
`efficient_q4`. The tokenizer must use data schema 2 and
`data_abi_id = worldless_transformer/gsp512_c256_w64_v1`. The generator
validates the tokenizer with the Python implementation, copies
the canonical artifact and its tokenizer-ID SHA-256 into the output,
regenerates all 510-piece trie states and runtime RMS/exp constants, and emits
projection-level `/compute` functions and bank-specific logits/argmax functions
for the selected architecture. It refuses to overwrite an output directory. The
output excludes the test-only
`data/transformer/function/fixture` and `data/worldless_lab` trees, so it
contains neither embedded weights nor fixture adapters.

## Command-chain budget

Inference is synchronous and one invocation processes every prefix/model
position, so command quota grows with the tokenized prefix and requested
generation. The full 256-position context does not fit Minecraft's default
`minecraft:max_command_sequence_length` value of 65,536. Setup, activation,
and inference are separate calls and must each receive an adequate limit.

For capacity planning, let `R` be the regular-prefix piece count. Unless EOS
ends generation early, the number of evaluated model positions is
`P = R + max_new_tokens`: the implicit BOS adds one position and the final
returned token removes one forward pass. Per-position cost also grows until the
local-attention window reaches 64 keys. Measure the concrete artifact, prefix,
and generation length, then configure
`minecraft:max_command_sequence_length` above the observed inference quota.
Budget setup and activation separately.
String input traversal also depends on UTF-16 input length, so there is no
artifact-independent text-entry limit below the model's numerical context.

Measure a production model and request with the release benchmark path:

```sh
cargo run --release -p worldless-lab -- benchmark \
  --pack OUTPUT_DIR --model-storage MODEL_STORAGE_DAT --entry text \
  --request '{prefix:"Once",max_new_tokens:1}' \
  --warmup 5 --samples 30 --quota COMMAND_LIMIT --format json
```
