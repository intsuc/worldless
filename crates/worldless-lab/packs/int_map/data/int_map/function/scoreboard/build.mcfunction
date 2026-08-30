execute store result storage int_map:state work.macro.entry_index int 1 run scoreboard players get #entry_index int_map
function int_map:scoreboard/load_entry.macro with storage int_map:state work.macro
execute store result storage int_map:state work.macro.key int 1 run scoreboard players get #entry_key int_map
function int_map:scoreboard/put.macro with storage int_map:state work.macro
scoreboard players add #entry_index int_map 1
execute if score #entry_index int_map < #entry_length int_map run return run function int_map:scoreboard/build
