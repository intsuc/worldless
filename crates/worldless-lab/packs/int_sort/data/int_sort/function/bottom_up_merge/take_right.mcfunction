execute store result storage int_sort:state work.macro.index int 1 run scoreboard players get #j int_sort
function int_sort:bottom_up_merge/append.macro with storage int_sort:state work.macro
scoreboard players add #j int_sort 1
return run function int_sort:bottom_up_merge/merge
