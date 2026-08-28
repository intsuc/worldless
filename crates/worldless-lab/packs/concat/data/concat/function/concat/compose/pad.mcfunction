data modify storage concat: tokens append value ""
scoreboard players add #width concat 1
execute if score #width concat < #capacity concat run function concat:concat/compose/pad
