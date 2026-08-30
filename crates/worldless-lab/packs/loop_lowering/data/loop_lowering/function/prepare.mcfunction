scoreboard objectives add loop_lowering dummy
scoreboard players set #valid loop_lowering 1

data remove storage loop_lowering:validation request
data modify storage loop_lowering:validation request set from storage worldless_lab:loop_lowering/input
data remove storage loop_lowering:validation request.iterations
data remove storage loop_lowering:validation request.seed
scoreboard players set #remaining_fields loop_lowering -1
execute store result score #remaining_fields loop_lowering run data get storage loop_lowering:validation request
execute unless score #remaining_fields loop_lowering matches 0 run scoreboard players set #valid loop_lowering 0

scoreboard players set #input_iterations loop_lowering 0
scoreboard players set #input_seed loop_lowering 0
execute store result score #input_iterations loop_lowering run data get storage worldless_lab:loop_lowering/input iterations
execute store result score #input_seed loop_lowering run data get storage worldless_lab:loop_lowering/input seed
data modify storage loop_lowering:validation scalars set value {iterations:0,seed:0}
execute store result storage loop_lowering:validation scalars.iterations int 1 run scoreboard players get #input_iterations loop_lowering
execute store result storage loop_lowering:validation scalars.seed int 1 run scoreboard players get #input_seed loop_lowering
function loop_lowering:validate_scalars.macro with storage loop_lowering:validation scalars

scoreboard players set #allowed_iterations loop_lowering 0
execute if score #input_iterations loop_lowering matches 0 run scoreboard players set #allowed_iterations loop_lowering 1
execute if score #input_iterations loop_lowering matches 1 run scoreboard players set #allowed_iterations loop_lowering 1
execute if score #input_iterations loop_lowering matches 3..5 run scoreboard players set #allowed_iterations loop_lowering 1
execute if score #input_iterations loop_lowering matches 15..17 run scoreboard players set #allowed_iterations loop_lowering 1
execute if score #input_iterations loop_lowering matches 64 run scoreboard players set #allowed_iterations loop_lowering 1
execute if score #input_iterations loop_lowering matches 256 run scoreboard players set #allowed_iterations loop_lowering 1
execute unless score #allowed_iterations loop_lowering matches 1 run scoreboard players set #valid loop_lowering 0
execute unless score #valid loop_lowering matches 1 run return fail

data modify storage loop_lowering:state work set value {result:{iterations:0,value:0,checksum:0},macro:{iterations:0,variant:""}}
execute store result storage loop_lowering:state work.macro.iterations int 1 run scoreboard players get #input_iterations loop_lowering
scoreboard players operation #requested loop_lowering = #input_iterations loop_lowering
scoreboard players operation #remaining loop_lowering = #input_iterations loop_lowering
scoreboard players set #executed loop_lowering 0
scoreboard players operation #value loop_lowering = #input_seed loop_lowering
scoreboard players set #checksum loop_lowering 1
scoreboard players set #lcg_multiplier loop_lowering 1664525
scoreboard players set #lcg_addend loop_lowering 1013904223
scoreboard players set #checksum_multiplier loop_lowering 31
return 1
