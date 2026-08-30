data modify storage int_map:state work.table set value {}
scoreboard players set #entry_index int_map 0
execute if score #entry_index int_map < #entry_length int_map run function int_map:nbt_compound/build
data modify storage int_map:state work.macro.variant set value "nbt_compound"
function int_map:query/run
