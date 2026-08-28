scoreboard players operation #capacity concat *= #two concat
scoreboard players add #levels concat 1
execute if score #capacity concat < #width concat run function concat:concat/compose/grow
