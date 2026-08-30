execute store result storage int_sort:state work.macro.i int 1 run scoreboard players get #i int_sort
function int_sort:insertion/load_key.macro with storage int_sort:state work.macro

scoreboard players operation #j int_sort = #i int_sort
scoreboard players remove #j int_sort 1
function int_sort:insertion/scan

scoreboard players operation #destination int_sort = #j int_sort
scoreboard players add #destination int_sort 1
execute store result storage int_sort:state work.macro.destination int 1 run scoreboard players get #destination int_sort
execute if score #destination int_sort < #i int_sort run function int_sort:insertion/relocate.macro with storage int_sort:state work.macro

scoreboard players add #i int_sort 1
execute if score #i int_sort < #length int_sort run return run function int_sort:insertion/outer
