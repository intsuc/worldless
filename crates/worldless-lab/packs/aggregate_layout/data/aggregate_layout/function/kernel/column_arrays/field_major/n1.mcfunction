execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout.x[0]
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout.x[0] int 1 run scoreboard players get #value aggregate_layout
execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout.y[0]
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout.y[0] int 1 run scoreboard players get #value aggregate_layout
execute store result score #value aggregate_layout run data get storage aggregate_layout:state work.layout.z[0]
scoreboard players operation #value aggregate_layout *= #factor aggregate_layout
scoreboard players add #value aggregate_layout 7
execute store result storage aggregate_layout:state work.layout.z[0] int 1 run scoreboard players get #value aggregate_layout
