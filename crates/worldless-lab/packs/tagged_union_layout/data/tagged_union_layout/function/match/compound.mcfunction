execute store result score #decoded_tag tag_union run data get storage tagged_union_layout:state work.cell.tag
execute if score #decoded_tag tag_union matches 0 run function tagged_union_layout:handle/compound/w2
execute if score #decoded_tag tag_union matches 1 run function tagged_union_layout:handle/compound/w0
execute if score #decoded_tag tag_union matches 2 run function tagged_union_layout:handle/compound/w1
execute if score #decoded_tag tag_union matches 3 run function tagged_union_layout:handle/compound/w2
