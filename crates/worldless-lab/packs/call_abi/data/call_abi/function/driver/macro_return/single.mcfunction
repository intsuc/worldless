execute store result score #source_a call_abi run data get storage worldless_lab:call_abi/input a[0]
execute store result score #source_b call_abi run data get storage worldless_lab:call_abi/input b[0]
execute store result storage call_abi:state work.frame.a int 1 run scoreboard players get #source_a call_abi
execute store result storage call_abi:state work.frame.b int 1 run scoreboard players get #source_b call_abi
execute store result score #result call_abi run function call_abi:leaf/macro_return with storage call_abi:state work.frame
function call_abi:fold
