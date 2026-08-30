scoreboard players set #checksum dynamic_vector 1
scoreboard players set #index dynamic_vector 0
execute if score #length dynamic_vector matches 0 run return 1
return run function dynamic_vector:checksum/next
