execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout[0].x
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout[0].x int 1 run scoreboard players get #value aggregate_layout
execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout[0].y
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout[0].y int 1 run scoreboard players get #value aggregate_layout
execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout[0].z
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout[0].z int 1 run scoreboard players get #value aggregate_layout
