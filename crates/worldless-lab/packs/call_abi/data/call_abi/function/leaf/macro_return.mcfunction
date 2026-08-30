$scoreboard players set #local call_abi $(a)
scoreboard players operation #local call_abi *= #factor call_abi
$scoreboard players add #local call_abi $(b)
scoreboard players operation #local call_abi *= #factor call_abi
scoreboard players add #local call_abi 7
return run scoreboard players get #local call_abi
