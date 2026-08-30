execute unless score #j int_sort matches 0.. run return 0
execute store result storage int_sort:state work.macro.j int 1 run scoreboard players get #j int_sort
function int_sort:insertion/load_current.macro with storage int_sort:state work.macro
execute unless score #current int_sort > #key int_sort run return 0
scoreboard players remove #j int_sort 1
return run function int_sort:insertion/scan
