scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #head result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r1 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r2 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r3 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r4 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r5 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r6 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players operation #state result_abi *= #lcg_multiplier result_abi
scoreboard players operation #state result_abi += #lcg_addend result_abi
scoreboard players operation #r7 result_abi = #state result_abi
scoreboard players add #values result_abi 1
scoreboard players add #calls result_abi 1
return run scoreboard players get #head result_abi
