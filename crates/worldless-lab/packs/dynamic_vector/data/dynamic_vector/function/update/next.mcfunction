scoreboard players operation #index dynamic_vector = #state dynamic_vector
scoreboard players operation #index dynamic_vector %= #length dynamic_vector
execute if score #index dynamic_vector matches ..-1 run scoreboard players operation #index dynamic_vector += #length dynamic_vector
scoreboard players operation #state dynamic_vector *= #lcg_multiplier dynamic_vector
scoreboard players operation #state dynamic_vector += #lcg_addend dynamic_vector
execute unless function dynamic_vector:dispatch/read run return fail
scoreboard players operation #value dynamic_vector *= #factor dynamic_vector
scoreboard players operation #value dynamic_vector += #affine_addend dynamic_vector
execute store result storage dynamic_vector:state work.value int 1 run scoreboard players get #value dynamic_vector
execute unless function dynamic_vector:dispatch/write run return fail
scoreboard players remove #remaining dynamic_vector 1
execute if score #remaining dynamic_vector matches 1.. run return run function dynamic_vector:update/next
return 1
