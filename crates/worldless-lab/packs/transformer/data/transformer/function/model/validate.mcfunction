scoreboard players set #valid transformer 1
execute unless data storage transformer:model {abi:{schema:1,architecture_id:"worldless_transformer/relu2_alibi_gsp512_l4_d96_q6_kv1_h16_ff192_c256_w64_v1",tokenizer_kind:"greedy_string_piece",vocab_size:512,bos_id:510,eos_id:511}} run scoreboard players set #valid transformer 0
data modify storage transformer:validation root set from storage transformer:model
data remove storage transformer:validation root.abi
data remove storage transformer:validation root.weights
data remove storage transformer:validation root.biases
data remove storage transformer:validation root.shifts
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:validation root
execute unless score #actual transformer matches 0 run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model abi
execute unless score #actual transformer matches 7 run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights
execute unless score #actual transformer matches 25 run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts
execute unless score #actual transformer matches 25 run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model biases
execute unless score #actual transformer matches 0 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model biases
execute unless data storage transformer:validation {probe:{}} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model abi.tokenizer_id
execute unless score #actual transformer matches 8 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model abi.tokenizer_id
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."token_embedding.weight"
execute unless score #actual transformer matches 49152 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."token_embedding.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."token_embedding.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."token_embedding.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."token_embedding.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."token_embedding.weight"
function transformer:model/generated/validate_range_49152
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.attention.q_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.attention.q_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.attention.q_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.attention.q_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.attention.k_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.attention.k_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.attention.k_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.attention.k_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.attention.v_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.attention.v_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.attention.v_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.attention.v_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.attention.out_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.attention.out_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.attention.out_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.attention.out_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.ffn.up_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.ffn.up_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.ffn.up_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.ffn.up_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.0.ffn.down_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.0.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.0.ffn.down_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.0.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.0.ffn.down_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.0.ffn.down_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.attention.q_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.attention.q_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.attention.q_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.attention.q_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.attention.k_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.attention.k_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.attention.k_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.attention.k_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.attention.v_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.attention.v_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.attention.v_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.attention.v_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.attention.out_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.attention.out_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.attention.out_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.attention.out_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.ffn.up_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.ffn.up_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.ffn.up_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.ffn.up_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.1.ffn.down_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.1.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.1.ffn.down_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.1.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.1.ffn.down_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.1.ffn.down_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.attention.q_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.attention.q_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.attention.q_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.attention.q_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.attention.k_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.attention.k_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.attention.k_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.attention.k_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.attention.v_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.attention.v_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.attention.v_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.attention.v_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.attention.out_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.attention.out_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.attention.out_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.attention.out_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.ffn.up_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.ffn.up_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.ffn.up_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.ffn.up_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.2.ffn.down_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.2.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.2.ffn.down_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.2.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.2.ffn.down_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.2.ffn.down_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.attention.q_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.attention.q_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.attention.q_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.attention.q_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.attention.q_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.attention.k_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.attention.k_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.attention.k_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.attention.k_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.attention.k_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.attention.v_proj.weight"
execute unless score #actual transformer matches 1536 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.attention.v_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.attention.v_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.attention.v_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.attention.v_proj.weight"
function transformer:model/generated/validate_range_1536
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.attention.out_proj.weight"
execute unless score #actual transformer matches 9216 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.attention.out_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.attention.out_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.attention.out_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.attention.out_proj.weight"
function transformer:model/generated/validate_range_9216
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.ffn.up_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.ffn.up_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.ffn.up_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.ffn.up_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.ffn.up_proj.weight"
function transformer:model/generated/validate_range_18432
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model weights."blocks.3.ffn.down_proj.weight"
execute unless score #actual transformer matches 18432 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model weights."blocks.3.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[B;]} run scoreboard players set #valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:model shifts."blocks.3.ffn.down_proj.weight"
execute unless score #actual transformer matches 1 run scoreboard players set #valid transformer 0
data modify storage transformer:validation probe set from storage transformer:model shifts."blocks.3.ffn.down_proj.weight"
data remove storage transformer:validation probe[]
execute unless data storage transformer:validation {probe:[I;]} run scoreboard players set #valid transformer 0
execute store result score #shift transformer run data get storage transformer:model shifts."blocks.3.ffn.down_proj.weight"[0]
execute unless score #shift transformer matches 0..30 run scoreboard players set #valid transformer 0
data modify storage transformer:validation matrix set from storage transformer:model weights."blocks.3.ffn.down_proj.weight"
function transformer:model/generated/validate_range_18432
execute store result score #shift transformer run data get storage transformer:model shifts."token_embedding.weight"[0]
execute unless score #shift transformer matches 0 run scoreboard players set #valid transformer 0
execute unless score #valid transformer matches 1 run return 0
return run scoreboard players get #valid transformer
