execute unless score #entry_index int_map matches 0.. run return 0
execute store result storage int_map:state work.macro.entry_index int 1 run scoreboard players get #entry_index int_map
function int_map:linear_scan/load_key.macro with storage int_map:state work.macro
execute if score #entry_key int_map = #query int_map run return run function int_map:linear_scan/hit
scoreboard players remove #entry_index int_map 1
return run function int_map:linear_scan/scan
