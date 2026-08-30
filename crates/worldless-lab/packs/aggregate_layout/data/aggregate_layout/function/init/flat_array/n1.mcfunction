data modify storage aggregate_layout:state work set value {layout:[I;0,0,0],result:{checksum:0}}
scoreboard players operation #generator aggregate_layout = #seed aggregate_layout
scoreboard players set #checksum aggregate_layout 1
execute store result storage aggregate_layout:state work.layout[0] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
execute store result storage aggregate_layout:state work.layout[1] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
execute store result storage aggregate_layout:state work.layout[2] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
