execute store result storage int_map:state work.macro.query_index int 1 run scoreboard players get #query_index int_map
function int_map:query/load.macro with storage int_map:state work.macro
execute store result storage int_map:state work.macro.key int 1 run scoreboard players get #query int_map

scoreboard players set #found int_map 0
scoreboard players set #value int_map 0
function int_map:query/lookup.macro with storage int_map:state work.macro
function int_map:query/emit

scoreboard players add #query_index int_map 1
execute if score #query_index int_map < #query_length int_map run return run function int_map:query/next
