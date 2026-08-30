scoreboard players operation #remaining dynamic_vector = #target_length dynamic_vector
scoreboard players operation #state dynamic_vector = #seed dynamic_vector
execute if score #remaining dynamic_vector matches 1.. unless function dynamic_vector:fill/next run return fail
execute if score #workload dynamic_vector matches 2 unless function dynamic_vector:update/setup run return fail
execute if score #workload dynamic_vector matches 3 unless function dynamic_vector:churn/setup run return fail
return run function dynamic_vector:checksum/setup
