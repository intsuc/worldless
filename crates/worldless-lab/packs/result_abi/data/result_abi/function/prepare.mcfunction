scoreboard objectives add result_abi dummy
scoreboard players set #valid result_abi 1

data remove storage result_abi:validation request
data modify storage result_abi:validation request set from storage worldless_lab:result_abi/input
data remove storage result_abi:validation request.width
data remove storage result_abi:validation request.seed
scoreboard players set #remaining_fields result_abi -1
execute store result score #remaining_fields result_abi run data get storage result_abi:validation request
execute unless score #remaining_fields result_abi matches 0 run scoreboard players set #valid result_abi 0

scoreboard players set #input_width result_abi 0
scoreboard players set #input_seed result_abi 0
execute store result score #input_width result_abi run data get storage worldless_lab:result_abi/input width
execute store result score #input_seed result_abi run data get storage worldless_lab:result_abi/input seed
data modify storage result_abi:validation scalars set value {width:0,seed:0}
execute store result storage result_abi:validation scalars.width int 1 run scoreboard players get #input_width result_abi
execute store result storage result_abi:validation scalars.seed int 1 run scoreboard players get #input_seed result_abi
function result_abi:validate_scalars.macro with storage result_abi:validation scalars

scoreboard players set #allowed_width result_abi 0
execute if score #input_width result_abi matches 1 run scoreboard players set #allowed_width result_abi 1
execute if score #input_width result_abi matches 2 run scoreboard players set #allowed_width result_abi 1
execute if score #input_width result_abi matches 4 run scoreboard players set #allowed_width result_abi 1
execute if score #input_width result_abi matches 8 run scoreboard players set #allowed_width result_abi 1
execute if score #input_width result_abi matches 16 run scoreboard players set #allowed_width result_abi 1
execute unless score #allowed_width result_abi matches 1 run scoreboard players set #valid result_abi 0
execute unless score #valid result_abi matches 1 run return fail

data modify storage result_abi:state work set value {frames:[],channel:{},macro:{width:0,variant:""},result:{width:0,checksum:0}}
execute store result storage result_abi:state work.macro.width int 1 run scoreboard players get #input_width result_abi
scoreboard players operation #requested_width result_abi = #input_width result_abi
scoreboard players operation #state result_abi = #input_seed result_abi
scoreboard players set #checksum result_abi 1
scoreboard players set #lcg_multiplier result_abi 1664525
scoreboard players set #lcg_addend result_abi 1013904223
scoreboard players set #checksum_multiplier result_abi 31
scoreboard players set #calls result_abi 0
scoreboard players set #values result_abi 0
scoreboard players set #folds result_abi 0
scoreboard players set #call_target result_abi 31
scoreboard players operation #expected_values result_abi = #requested_width result_abi
scoreboard players operation #expected_values result_abi *= #call_target result_abi
return 1
