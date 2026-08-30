scoreboard objectives add indirect_access dummy
scoreboard players set #valid indirect_access 1

data remove storage indirect_access:validation request
data modify storage indirect_access:validation request set from storage worldless_lab:indirect_access/input
data remove storage indirect_access:validation request.values
data remove storage indirect_access:validation request.indices
scoreboard players set #remaining indirect_access -1
execute store result score #remaining indirect_access run data get storage indirect_access:validation request
execute unless score #remaining indirect_access matches 0 run scoreboard players set #valid indirect_access 0

function indirect_access:validate_array.macro {field:"values"}
function indirect_access:validate_array.macro {field:"indices"}

scoreboard players set #value_length indirect_access -1
scoreboard players set #index_length indirect_access -1
execute store result score #value_length indirect_access run data get storage worldless_lab:indirect_access/input values
execute store result score #index_length indirect_access run data get storage worldless_lab:indirect_access/input indices
execute unless score #value_length indirect_access matches 16 run scoreboard players set #valid indirect_access 0
execute unless score #index_length indirect_access matches 63 run scoreboard players set #valid indirect_access 0

execute if score #valid indirect_access matches 1 store success score #valid indirect_access run function indirect_access:validate_indices
execute unless score #valid indirect_access matches 1 run return fail

data modify storage indirect_access:state work set value {macro:{variant:"",index:0},result:{checksum:0}}
scoreboard players set #checksum indirect_access 1
scoreboard players set #factor indirect_access 31
return 1
