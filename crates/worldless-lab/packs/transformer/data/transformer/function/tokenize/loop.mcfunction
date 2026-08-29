data remove storage transformer:runtime state.best_id
data remove storage transformer:runtime state.best_remaining
data modify storage transformer:runtime state.scan set from storage transformer:runtime state.remaining
function transformer:tokenize/state/0
execute unless data storage transformer:runtime state.best_id run scoreboard players set #token_error transformer 1
execute if score #token_error transformer matches 1 run return 0
data modify storage transformer:runtime state.tokens append from storage transformer:runtime state.best_id
data modify storage transformer:runtime state.remaining set from storage transformer:runtime state.best_remaining
execute unless data storage transformer:runtime {state:{remaining:""}} run function transformer:tokenize/loop
