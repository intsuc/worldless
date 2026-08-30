scoreboard objectives add numeric_lowering dummy
scoreboard players set #valid numeric_lowering 1

data remove storage numeric_lowering:validation request
data modify storage numeric_lowering:validation request set from storage worldless_lab:numeric_lowering/input
data remove storage numeric_lowering:validation request.a
data remove storage numeric_lowering:validation request.b
scoreboard players set #remaining numeric_lowering -1
execute store result score #remaining numeric_lowering run data get storage numeric_lowering:validation request
execute unless score #remaining numeric_lowering matches 0 run scoreboard players set #valid numeric_lowering 0

function numeric_lowering:validate_array.macro {field:"a"}
function numeric_lowering:validate_array.macro {field:"b"}

scoreboard players set #a_length numeric_lowering -1
scoreboard players set #b_length numeric_lowering -1
execute store result score #a_length numeric_lowering run data get storage worldless_lab:numeric_lowering/input a
execute store result score #b_length numeric_lowering run data get storage worldless_lab:numeric_lowering/input b
execute unless score #a_length numeric_lowering = #b_length numeric_lowering run scoreboard players set #valid numeric_lowering 0
scoreboard players set #allowed_length numeric_lowering 0
execute if score #a_length numeric_lowering matches 1 run scoreboard players set #allowed_length numeric_lowering 1
execute if score #a_length numeric_lowering matches 4 run scoreboard players set #allowed_length numeric_lowering 1
execute if score #a_length numeric_lowering matches 16 run scoreboard players set #allowed_length numeric_lowering 1
execute if score #a_length numeric_lowering matches 64 run scoreboard players set #allowed_length numeric_lowering 1
execute unless score #allowed_length numeric_lowering matches 1 run scoreboard players set #valid numeric_lowering 0

execute unless score #valid numeric_lowering matches 1 run return fail
execute if score #a_length numeric_lowering matches 1 run function numeric_lowering:validate/range_1
execute if score #a_length numeric_lowering matches 4 run function numeric_lowering:validate/range_4
execute if score #a_length numeric_lowering matches 16 run function numeric_lowering:validate/range_16
execute if score #a_length numeric_lowering matches 64 run function numeric_lowering:validate/range_64
execute unless score #valid numeric_lowering matches 1 run return fail

data modify storage numeric_lowering:state work set value {result:{checksum:0}}
scoreboard players set #checksum numeric_lowering 1
scoreboard players set #factor numeric_lowering 31
return 1
