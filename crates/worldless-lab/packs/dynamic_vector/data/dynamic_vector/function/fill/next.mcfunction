execute store result storage dynamic_vector:state work.value int 1 run scoreboard players get #state dynamic_vector
execute unless function dynamic_vector:dispatch/push run return fail
scoreboard players add #length dynamic_vector 1
scoreboard players operation #state dynamic_vector *= #lcg_multiplier dynamic_vector
scoreboard players operation #state dynamic_vector += #lcg_addend dynamic_vector
scoreboard players remove #remaining dynamic_vector 1
execute if score #remaining dynamic_vector matches 1.. run return run function dynamic_vector:fill/next
return 1
