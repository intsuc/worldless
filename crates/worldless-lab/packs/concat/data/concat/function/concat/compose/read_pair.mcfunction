execute store result storage concat: left_index int 1 run scoreboard players get #index concat
scoreboard players add #index concat 1
execute store result storage concat: right_index int 1 run scoreboard players get #index concat
scoreboard players add #index concat 1
function concat:concat/compose/load_pair.macro with storage concat:
