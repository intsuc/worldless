execute store result score #width concat run data get storage concat: tokens
scoreboard players set #failed concat 0
execute if score #width concat matches 0 run scoreboard players set #failed concat 1
execute if score #width concat matches 2.. run function concat:concat/compose/setup
return run execute if score #failed concat matches 0
