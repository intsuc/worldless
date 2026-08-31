scoreboard objectives add i64_lowering dummy

data remove storage i64_lowering:validation request
execute store success score #valid i64_lowering run data modify storage i64_lowering:validation request set from storage worldless_lab:i64_lowering/input
data remove storage i64_lowering:validation request.x
data remove storage i64_lowering:validation request.y
data remove storage i64_lowering:validation request.step
data remove storage i64_lowering:validation request.rounds
execute store result score #remaining_fields i64_lowering run data get storage i64_lowering:validation request
execute unless score #remaining_fields i64_lowering matches 0 run scoreboard players set #valid i64_lowering 0

function i64_lowering:validate_long.macro {field:"x"}
function i64_lowering:validate_long.macro {field:"y"}
function i64_lowering:validate_long.macro {field:"step"}

execute store result score #rounds i64_lowering run data get storage worldless_lab:i64_lowering/input rounds
data modify storage i64_lowering:validation scalars set value {rounds:0}
execute store result storage i64_lowering:validation scalars.rounds int 1 run scoreboard players get #rounds i64_lowering
function i64_lowering:validate_rounds.macro with storage i64_lowering:validation scalars

scoreboard players set #allowed_rounds i64_lowering 0
execute if score #rounds i64_lowering matches 1 run scoreboard players set #allowed_rounds i64_lowering 1
execute if score #rounds i64_lowering matches 8 run scoreboard players set #allowed_rounds i64_lowering 1
execute if score #rounds i64_lowering matches 64 run scoreboard players set #allowed_rounds i64_lowering 1
execute unless score #allowed_rounds i64_lowering matches 1 run scoreboard players set #valid i64_lowering 0

execute unless score #valid i64_lowering matches 1 run return fail
data modify storage i64_lowering:state work set value {result:{}}
data modify storage i64_lowering:state macro set value {rounds:0,variant:""}
execute store result storage i64_lowering:state macro.rounds int 1 run scoreboard players get #rounds i64_lowering
data modify storage i64_lowering:validation wrapper set value [I;0]

scoreboard players set #less_count i64_lowering 0

data modify storage i64_lowering:validation source set from storage worldless_lab:i64_lowering/input x
function i64_lowering:split
scoreboard players operation #x_in_high i64_lowering = #split_high i64_lowering
scoreboard players operation #x_in_low i64_lowering = #split_low i64_lowering

data modify storage i64_lowering:validation source set from storage worldless_lab:i64_lowering/input y
function i64_lowering:split
scoreboard players operation #y_in_high i64_lowering = #split_high i64_lowering
scoreboard players operation #y_in_low i64_lowering = #split_low i64_lowering

data modify storage i64_lowering:validation source set from storage worldless_lab:i64_lowering/input step
function i64_lowering:split
scoreboard players operation #step_in_high i64_lowering = #split_high i64_lowering
scoreboard players operation #step_in_low i64_lowering = #split_low i64_lowering
return 1
