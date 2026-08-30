scoreboard players operation #local call_abi = #arg_a call_abi
scoreboard players operation #local call_abi *= #factor call_abi
scoreboard players operation #local call_abi += #arg_b call_abi
scoreboard players operation #local call_abi *= #factor call_abi
scoreboard players add #local call_abi 7
return run scoreboard players get #local call_abi
