data remove storage concat: next_escape
function concat:concat/compose/double_escape.macro with storage concat:
execute unless data storage concat: next_escape run scoreboard players set #failed concat 1
execute if score #failed concat matches 0 run data modify storage concat: escape set from storage concat: next_escape
execute if score #failed concat matches 0 run data remove storage concat: next_escape
execute if score #failed concat matches 0 run scoreboard players remove #levels concat 1
execute if score #failed concat matches 0 if score #levels concat matches 1.. run function concat:concat/compose/double_escape
