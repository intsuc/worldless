scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r0 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players add #calls result_abi 1
