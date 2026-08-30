scoreboard objectives add dynamic_vector dummy
scoreboard players set #valid dynamic_vector 1
data remove storage dynamic_vector:validation request
data modify storage dynamic_vector:validation request set from storage worldless_lab:dynamic_vector/input
data remove storage dynamic_vector:validation request.length
data remove storage dynamic_vector:validation request.seed
data remove storage dynamic_vector:validation request.workload
scoreboard players set #remaining_fields dynamic_vector -1
execute store result score #remaining_fields dynamic_vector run data get storage dynamic_vector:validation request
execute unless score #remaining_fields dynamic_vector matches 0 run scoreboard players set #valid dynamic_vector 0
scoreboard players set #target_length dynamic_vector -1
scoreboard players set #seed dynamic_vector 0
execute store result score #target_length dynamic_vector run data get storage worldless_lab:dynamic_vector/input length
execute store result score #seed dynamic_vector run data get storage worldless_lab:dynamic_vector/input seed
data modify storage dynamic_vector:validation scalars set value {length:0,seed:0}
execute store result storage dynamic_vector:validation scalars.length int 1 run scoreboard players get #target_length dynamic_vector
execute store result storage dynamic_vector:validation scalars.seed int 1 run scoreboard players get #seed dynamic_vector
function dynamic_vector:validate_scalars.macro with storage dynamic_vector:validation scalars
execute unless score #target_length dynamic_vector matches 0..256 run scoreboard players set #valid dynamic_vector 0
scoreboard players set #workload dynamic_vector 0
execute if data storage worldless_lab:dynamic_vector/input {workload:"build"} run scoreboard players set #workload dynamic_vector 1
execute if data storage worldless_lab:dynamic_vector/input {workload:"random_update"} run scoreboard players set #workload dynamic_vector 2
execute if data storage worldless_lab:dynamic_vector/input {workload:"churn"} run scoreboard players set #workload dynamic_vector 3
execute unless score #workload dynamic_vector matches 1..3 run scoreboard players set #valid dynamic_vector 0
execute if score #workload dynamic_vector matches 2 if score #target_length dynamic_vector matches 0 run scoreboard players set #valid dynamic_vector 0
execute unless score #valid dynamic_vector matches 1 run return fail
scoreboard players set #lcg_multiplier dynamic_vector 1664525
scoreboard players set #lcg_addend dynamic_vector 1013904223
scoreboard players set #factor dynamic_vector 31
scoreboard players set #affine_addend dynamic_vector 7
scoreboard players set #page_size dynamic_vector 16
scoreboard players set #two dynamic_vector 2
return 1
