# Integer decoder transformer pack

This data pack executes a fixed decoder-only transformer with integer operations
in Worldless and Minecraft. The architecture is
`worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1`:
four pre-norm layers, width 96, six query heads and one width-16 KV head,
ReLU-squared FFN width 192, ALiBi, context 256, local attention window 64, and
a tied 512-token embedding/unembedding.

## Runtime contract

Run `transformer:setup` once after the pack is loaded. It owns the scoreboard
objective and the runtime RMS/softmax tables; it does not install model weights.

Install one model compound at command storage `transformer:model`. Its only
top-level fields are `abi`, `weights`, `biases`, and `shifts`.
`weights` contains the exact flattened matrices owned by
`worldless_transformer.spec.expected_weight_shapes()` as `ByteArrayTag`
values in -127..127. Every corresponding `shifts` value is an
`IntArrayTag[1]`; `biases` is an empty `CompoundTag`; and
`abi.tokenizer_id` is an `IntArrayTag[8]`. The exporter is the canonical
owner of these NBT types. Runtime validation additionally rejects unknown or
missing fields, wrong shapes, -128 weights, and shifts outside 0..30.

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

1. invalid model artifact;
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
  TOKENIZER_JSON OUTPUT_DIR
```

The generator validates the tokenizer with the Python implementation, copies
the canonical artifact and its tokenizer-ID SHA-256 into the output,
regenerates all 510-piece trie states and runtime RMS/exp constants, and emits
row-level `/compute`
functions for the fixed architecture. It refuses to overwrite an output
directory. The output excludes the test-only
`data/transformer/function/fixture` and `data/worldless_lab` trees, so it
contains neither embedded weights nor fixture adapters.

## Command-chain budget

Inference is synchronous and one invocation processes every prefix/model
position, so command quota grows with the tokenized prefix and requested
generation. The full 256-position context does not fit Minecraft's default
`maxCommandChainLength=65536`. After a separate setup call, a measured
three-position public request uses 65,917 commands through the text entry and
65,897 through the token entry. One
isolated position with all 64 attention keys active uses 76,703 commands in
`transformer:core/process_position` alone. Thus the default budget can
accommodate some short prefixes but not arbitrary longer prefixes, and it
cannot execute even one full-window position.

For capacity planning, let `R` be the regular-prefix piece count. Unless EOS
ends generation early, the number of evaluated model positions is
`P = R + max_new_tokens`: the implicit BOS adds one position and the final
returned token removes one forward pass. A conservative model-core estimate is
`76,703 * P` commands, because pre-saturation positions use no more attention
keys than the measured full-window position; at `P=256` that term alone is
19,635,968. Configure
`maxCommandChainLength` above that core budget plus model validation, request
validation, and generated-trie traversal for the concrete artifact and prefix.
String input traversal also depends on UTF-16 input length, so there is no
artifact-independent text-entry limit below the model's numerical context.
