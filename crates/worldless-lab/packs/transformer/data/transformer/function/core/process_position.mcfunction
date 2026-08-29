execute store result storage transformer:runtime state.macro.index int 1 run scoreboard players get #position transformer
scoreboard players set #token transformer -1
function transformer:core/load_context_token.macro with storage transformer:runtime state.macro
function transformer:core/embed
scoreboard players set #layer transformer 0
function transformer:core/layer
function transformer:core/rms/run
function transformer:core/logits/run

scoreboard players operation #last_position transformer = #token_count transformer
scoreboard players remove #last_position transformer 1
execute if score #position transformer < #last_position transformer run return run function transformer:core/advance_position

execute store result storage transformer:runtime state.scalar int 1 run scoreboard players get #next_token transformer
data modify storage transformer:runtime state.generated append from storage transformer:runtime state.scalar
scoreboard players add #generated_count transformer 1
execute if score #next_token transformer = #eos transformer run return 1
execute if score #generated_count transformer >= #max_new transformer run return 1
data modify storage transformer:runtime state.tokens append from storage transformer:runtime state.scalar
scoreboard players add #token_count transformer 1
scoreboard players add #position transformer 1
return run function transformer:core/process_position
