execute unless function dynamic_vector:dispatch/pop run return fail
scoreboard players remove #length dynamic_vector 1
scoreboard players remove #remaining dynamic_vector 1
execute if score #remaining dynamic_vector matches 1.. run return run function dynamic_vector:churn/pop_next
return 1
