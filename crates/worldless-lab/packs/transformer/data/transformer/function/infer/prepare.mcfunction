scoreboard players set #layer transformer 0
function transformer:infer/init_cache
scoreboard players set #position transformer 0
scoreboard players set #generated_count transformer 0
execute store result score #token_count transformer run data get storage transformer:runtime state.tokens
function transformer:core/process_position
