scoreboard objectives add call_abi dummy
scoreboard players set #valid call_abi 1

data remove storage call_abi:validation request
data modify storage call_abi:validation request set from storage worldless_lab:call_abi/input
data remove storage call_abi:validation request.a
data remove storage call_abi:validation request.b
scoreboard players set #remaining call_abi -1
execute store result score #remaining call_abi run data get storage call_abi:validation request
execute unless score #remaining call_abi matches 0 run scoreboard players set #valid call_abi 0

function call_abi:validate_array.macro {field:"a"}
function call_abi:validate_array.macro {field:"b"}

scoreboard players set #a_length call_abi -1
scoreboard players set #b_length call_abi -1
execute store result score #a_length call_abi run data get storage worldless_lab:call_abi/input a
execute store result score #b_length call_abi run data get storage worldless_lab:call_abi/input b
execute unless score #a_length call_abi = #b_length call_abi run scoreboard players set #valid call_abi 0
scoreboard players set #allowed_length call_abi 0
execute if score #a_length call_abi matches 1 run scoreboard players set #allowed_length call_abi 1
execute if score #a_length call_abi matches 63 run scoreboard players set #allowed_length call_abi 1
execute unless score #allowed_length call_abi matches 1 run scoreboard players set #valid call_abi 0

execute unless score #valid call_abi matches 1 run return fail

data modify storage call_abi:state work set value {frame:{a:0,b:0},result:{checksum:0}}
scoreboard players set #checksum call_abi 1
scoreboard players set #factor call_abi 31
scoreboard players set #length call_abi 0
scoreboard players operation #length call_abi = #a_length call_abi
return 1
