function concat:concat/compose/read_pair
data remove storage concat: merged
function concat:concat/compose/merge.macro with storage concat:
execute unless data storage concat: merged run scoreboard players set #failed concat 1
execute if score #failed concat matches 0 run data modify storage concat: next append from storage concat: merged
execute if score #failed concat matches 0 run data remove storage concat: merged
execute if score #failed concat matches 0 if score #index concat < #width concat run function concat:concat/compose/pair
