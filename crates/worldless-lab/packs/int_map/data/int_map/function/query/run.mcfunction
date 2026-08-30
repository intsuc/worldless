scoreboard players set #query_index int_map 0
execute if score #query_index int_map < #query_length int_map run function int_map:query/next
