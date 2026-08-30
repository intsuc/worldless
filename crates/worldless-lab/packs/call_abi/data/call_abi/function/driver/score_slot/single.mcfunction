execute store result score #source_a call_abi run data get storage worldless_lab:call_abi/input a[0]
execute store result score #source_b call_abi run data get storage worldless_lab:call_abi/input b[0]
scoreboard players operation #arg_a call_abi = #source_a call_abi
scoreboard players operation #arg_b call_abi = #source_b call_abi
function call_abi:leaf/score_slot
function call_abi:fold
