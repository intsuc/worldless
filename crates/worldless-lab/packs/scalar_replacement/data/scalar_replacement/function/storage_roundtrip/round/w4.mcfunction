execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[0]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[0] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[1]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[1] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[2]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[2] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[3]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[3] int 1 run scoreboard players get #value scalar_replace
