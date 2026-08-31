data modify storage tagged_union_layout:state work.cell set value [I;0,0,0]
execute store result storage tagged_union_layout:state work.cell[1] int 1 run scoreboard players get #p0 tag_union
execute store result storage tagged_union_layout:state work.cell[2] int 1 run scoreboard players get #p1 tag_union
