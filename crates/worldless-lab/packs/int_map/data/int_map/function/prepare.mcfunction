scoreboard objectives add int_map dummy
scoreboard players set #valid int_map 1

data remove storage int_map:validation request
data modify storage int_map:validation request set from storage worldless_lab:int_map/input
data remove storage int_map:validation request.keys
data remove storage int_map:validation request.values
data remove storage int_map:validation request.queries
scoreboard players set #remaining int_map -1
execute store result score #remaining int_map run data get storage int_map:validation request
execute unless score #remaining int_map matches 0 run scoreboard players set #valid int_map 0

function int_map:validate_array.macro {field:"keys"}
function int_map:validate_array.macro {field:"values"}
function int_map:validate_array.macro {field:"queries"}

scoreboard players set #entry_length int_map 0
scoreboard players set #value_length int_map 0
scoreboard players set #query_length int_map 0
execute store result score #entry_length int_map run data get storage worldless_lab:int_map/input keys
execute store result score #value_length int_map run data get storage worldless_lab:int_map/input values
execute store result score #query_length int_map run data get storage worldless_lab:int_map/input queries
execute unless score #entry_length int_map = #value_length int_map run scoreboard players set #valid int_map 0
execute unless score #valid int_map matches 1 run return 0

data modify storage int_map:state work set value {keys:[I;],values:[I;],queries:[I;],output:{found:[B;],values:[I;]},macro:{variant:""},entry:{key:0,value:0},result:{found:0b,value:0}}
data modify storage int_map:state work.keys set from storage worldless_lab:int_map/input keys
data modify storage int_map:state work.values set from storage worldless_lab:int_map/input values
data modify storage int_map:state work.queries set from storage worldless_lab:int_map/input queries
return 1
