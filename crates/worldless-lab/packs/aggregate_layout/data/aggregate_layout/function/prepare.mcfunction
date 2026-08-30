scoreboard objectives add aggregate_layout dummy
scoreboard players set #valid aggregate_layout 1

data remove storage aggregate_layout:validation request
data modify storage aggregate_layout:validation request set from storage worldless_lab:aggregate_layout/input
data remove storage aggregate_layout:validation request.length
data remove storage aggregate_layout:validation request.seed
data remove storage aggregate_layout:validation request.order
scoreboard players set #remaining aggregate_layout -1
execute store result score #remaining aggregate_layout run data get storage aggregate_layout:validation request
execute unless score #remaining aggregate_layout matches 0 run scoreboard players set #valid aggregate_layout 0

scoreboard players set #length aggregate_layout 0
scoreboard players set #seed aggregate_layout 0
execute store result score #length aggregate_layout run data get storage worldless_lab:aggregate_layout/input length
execute store result score #seed aggregate_layout run data get storage worldless_lab:aggregate_layout/input seed
data modify storage aggregate_layout:validation scalars set value {length:0,seed:0}
execute store result storage aggregate_layout:validation scalars.length int 1 run scoreboard players get #length aggregate_layout
execute store result storage aggregate_layout:validation scalars.seed int 1 run scoreboard players get #seed aggregate_layout
function aggregate_layout:validate_scalars.macro with storage aggregate_layout:validation scalars

scoreboard players set #allowed_length aggregate_layout 0
execute if score #length aggregate_layout matches 1 run scoreboard players set #allowed_length aggregate_layout 1
execute if score #length aggregate_layout matches 16 run scoreboard players set #allowed_length aggregate_layout 1
execute if score #length aggregate_layout matches 64 run scoreboard players set #allowed_length aggregate_layout 1
execute if score #length aggregate_layout matches 128 run scoreboard players set #allowed_length aggregate_layout 1
execute unless score #allowed_length aggregate_layout matches 1 run scoreboard players set #valid aggregate_layout 0

scoreboard players set #order aggregate_layout 0
execute if data storage worldless_lab:aggregate_layout/input {order:"record_major"} run scoreboard players set #order aggregate_layout 1
execute if data storage worldless_lab:aggregate_layout/input {order:"field_major"} run scoreboard players set #order aggregate_layout 2
execute unless score #order aggregate_layout matches 1..2 run scoreboard players set #valid aggregate_layout 0

execute unless score #valid aggregate_layout matches 1 run return fail
scoreboard players set #lcg_factor aggregate_layout 1664525
scoreboard players set #factor aggregate_layout 31
return 1
