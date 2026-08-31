data modify storage tagged_union_layout:state work.cell set value {tag:3,payload:{p0:0,p1:0}}
execute store result storage tagged_union_layout:state work.cell.payload.p0 int 1 run scoreboard players get #p0 tag_union
execute store result storage tagged_union_layout:state work.cell.payload.p1 int 1 run scoreboard players get #p1 tag_union
