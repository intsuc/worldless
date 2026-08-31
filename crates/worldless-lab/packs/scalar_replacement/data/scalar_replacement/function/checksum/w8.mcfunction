scoreboard players set #checksum scalar_replace 1
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[0]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[1]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[2]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[3]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[4]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[5]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[6]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result score #value scalar_replace run data get storage scalar_replacement:state work.values[7]
scoreboard players operation #checksum scalar_replace *= #factor scalar_replace
scoreboard players operation #checksum scalar_replace += #value scalar_replace
execute store result storage scalar_replacement:state work.result.checksum int 1 run scoreboard players get #checksum scalar_replace
