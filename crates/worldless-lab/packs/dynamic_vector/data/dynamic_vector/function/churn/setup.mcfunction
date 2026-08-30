scoreboard players operation #remaining dynamic_vector = #target_length dynamic_vector
scoreboard players operation #remaining dynamic_vector /= #two dynamic_vector
execute if score #remaining dynamic_vector matches 1.. unless function dynamic_vector:churn/pop_next run return fail
scoreboard players operation #remaining dynamic_vector = #target_length dynamic_vector
scoreboard players operation #remaining dynamic_vector /= #two dynamic_vector
execute if score #remaining dynamic_vector matches 1.. run return run function dynamic_vector:fill/next
return 1
