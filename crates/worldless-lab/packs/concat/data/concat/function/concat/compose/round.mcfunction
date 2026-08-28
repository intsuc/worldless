data modify storage concat: next set value []
scoreboard players set #index concat 0
function concat:concat/compose/pair
execute if score #failed concat matches 0 run data modify storage concat: tokens set from storage concat: next
execute if score #failed concat matches 0 run data remove storage concat: next
execute if score #failed concat matches 0 run scoreboard players operation #width concat /= #two concat
execute if score #failed concat matches 0 if score #width concat matches 2.. run function concat:concat/compose/round
