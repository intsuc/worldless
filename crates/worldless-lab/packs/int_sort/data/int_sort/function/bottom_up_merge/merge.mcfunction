execute unless score #i int_sort < #mid int_sort unless score #j int_sort < #right int_sort run return 0
execute unless score #i int_sort < #mid int_sort run return run function int_sort:bottom_up_merge/take_right
execute unless score #j int_sort < #right int_sort run return run function int_sort:bottom_up_merge/take_left

execute store result storage int_sort:state work.macro.i int 1 run scoreboard players get #i int_sort
execute store result storage int_sort:state work.macro.j int 1 run scoreboard players get #j int_sort
function int_sort:bottom_up_merge/load_pair.macro with storage int_sort:state work.macro

execute if score #left_value int_sort <= #right_value int_sort run return run function int_sort:bottom_up_merge/take_left
return run function int_sort:bottom_up_merge/take_right
