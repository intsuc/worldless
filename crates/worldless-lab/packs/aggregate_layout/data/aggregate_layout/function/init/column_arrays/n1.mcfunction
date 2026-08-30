data modify storage aggregate_layout:state work set value {layout:{x:[I;0],y:[I;0],z:[I;0]},result:{checksum:0}}
scoreboard players operation #generator aggregate_layout = #seed aggregate_layout
scoreboard players set #checksum aggregate_layout 1
execute store result storage aggregate_layout:state work.layout.x[0] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
execute store result storage aggregate_layout:state work.layout.y[0] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
execute store result storage aggregate_layout:state work.layout.z[0] int 1 run scoreboard players get #generator aggregate_layout
scoreboard players operation #generator aggregate_layout *= #lcg_factor aggregate_layout
scoreboard players add #generator aggregate_layout 1013904223
