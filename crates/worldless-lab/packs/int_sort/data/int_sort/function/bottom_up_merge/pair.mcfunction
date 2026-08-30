scoreboard players operation #mid int_sort = #left int_sort
scoreboard players operation #mid int_sort += #width int_sort
scoreboard players operation #mid int_sort < #length int_sort

scoreboard players operation #right int_sort = #mid int_sort
scoreboard players operation #right int_sort += #width int_sort
scoreboard players operation #right int_sort < #length int_sort

scoreboard players operation #i int_sort = #left int_sort
scoreboard players operation #j int_sort = #mid int_sort
function int_sort:bottom_up_merge/merge

scoreboard players operation #left int_sort = #right int_sort
execute if score #left int_sort < #length int_sort run return run function int_sort:bottom_up_merge/pair
