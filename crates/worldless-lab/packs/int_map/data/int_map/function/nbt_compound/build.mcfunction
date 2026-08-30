execute store result storage int_map:state work.macro.entry_index int 1 run scoreboard players get #entry_index int_map
function int_map:nbt_compound/load_entry.macro with storage int_map:state work.macro
function int_map:nbt_compound/put.macro with storage int_map:state work.entry
scoreboard players add #entry_index int_map 1
execute if score #entry_index int_map < #entry_length int_map run return run function int_map:nbt_compound/build
