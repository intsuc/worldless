execute unless function dynamic_vector:dispatch/read run return fail
scoreboard players operation #checksum dynamic_vector *= #factor dynamic_vector
scoreboard players operation #checksum dynamic_vector += #value dynamic_vector
scoreboard players add #index dynamic_vector 1
execute if score #index dynamic_vector < #length dynamic_vector run return run function dynamic_vector:checksum/next
return 1
