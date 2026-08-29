scoreboard players set #request_valid transformer 1
data modify storage transformer:validation request set from storage transformer:request
data remove storage transformer:validation request.prefix_tokens
data remove storage transformer:validation request.max_new_tokens
data remove storage transformer:validation request.tokenizer_id
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:validation request
execute unless score #actual transformer matches 0 run scoreboard players set #request_valid transformer 0

data modify storage transformer:validation array_probe set from storage transformer:request prefix_tokens
data remove storage transformer:validation array_probe[]
execute unless data storage transformer:validation {array_probe:[I;]} run scoreboard players set #request_valid transformer 0
scoreboard players set #prefix_len transformer -1
execute store result score #prefix_len transformer run data get storage transformer:request prefix_tokens
execute unless score #prefix_len transformer matches 0..255 run scoreboard players set #request_valid transformer 0

data modify storage transformer:validation array_probe set from storage transformer:request tokenizer_id
data remove storage transformer:validation array_probe[]
execute unless data storage transformer:validation {array_probe:[I;]} run scoreboard players set #request_valid transformer 0
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:request tokenizer_id
execute unless score #actual transformer matches 8 run scoreboard players set #request_valid transformer 0

scoreboard players set #max_new transformer -1
execute store result score #max_new transformer run data get storage transformer:request max_new_tokens
execute unless score #max_new transformer matches 1..256 run scoreboard players set #request_valid transformer 0
execute store result storage transformer:validation macro.max_new_tokens int 1 run scoreboard players get #max_new transformer
function transformer:infer/validate_max_new_type.macro with storage transformer:validation macro
return run scoreboard players get #request_valid transformer
