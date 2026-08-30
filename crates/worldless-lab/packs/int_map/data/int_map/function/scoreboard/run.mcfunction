scoreboard objectives add int_map_values dummy
scoreboard players reset * int_map_values
scoreboard players set #entry_index int_map 0
execute if score #entry_index int_map < #entry_length int_map run function int_map:scoreboard/build
data modify storage int_map:state work.macro.variant set value "scoreboard"
function int_map:query/run
