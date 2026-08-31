scoreboard players operation #v0 scalar_replace *= #factor scalar_replace
scoreboard players add #v0 scalar_replace 7
scoreboard players operation #v1 scalar_replace *= #factor scalar_replace
scoreboard players add #v1 scalar_replace 7
scoreboard players operation #v2 scalar_replace *= #factor scalar_replace
scoreboard players add #v2 scalar_replace 7
scoreboard players operation #v3 scalar_replace *= #factor scalar_replace
scoreboard players add #v3 scalar_replace 7
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[4]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[4] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[5]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[5] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[6]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[6] int 1 run scoreboard players get #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[7]
scoreboard players operation #value scalar_replace *= #factor scalar_replace
scoreboard players add #value scalar_replace 7
execute store result storage scalar_replacement:state work.values[7] int 1 run scoreboard players get #value scalar_replace
