scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
execute store result storage result_abi:state work.frames[-1].v0 int 1 run scoreboard players get #state result_abi
scoreboard players add #values result_abi 1
scoreboard players add #calls result_abi 1
