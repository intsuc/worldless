execute store result score #local call_abi run data get storage call_abi:state work.frame.a
scoreboard players operation #local call_abi *= #factor call_abi
execute store result score #scratch call_abi run data get storage call_abi:state work.frame.b
scoreboard players operation #local call_abi += #scratch call_abi
scoreboard players operation #local call_abi *= #factor call_abi
scoreboard players add #local call_abi 7
return run scoreboard players get #local call_abi
