scoreboard players operation #end concat = #index concat
scoreboard players remove #end concat 1
execute store result storage concat: start int 1 run scoreboard players get #marker concat
execute store result storage concat: end int 1 run scoreboard players get #end concat
execute if score #marker concat < #end concat run function concat:concat/decompose/append.macro with storage concat:

data modify storage concat: tokens append from storage concat: char
scoreboard players operation #marker concat = #index concat
