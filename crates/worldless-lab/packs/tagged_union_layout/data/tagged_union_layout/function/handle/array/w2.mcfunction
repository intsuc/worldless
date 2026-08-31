scoreboard players operation #arm tag_union = #decoded_tag tag_union
scoreboard players add #arm tag_union 1
execute store result score #value tag_union run data get storage tagged_union_layout:state work.cell[1]
scoreboard players operation #arm tag_union *= #factor_31 tag_union
scoreboard players operation #arm tag_union += #value tag_union
execute store result score #value tag_union run data get storage tagged_union_layout:state work.cell[2]
scoreboard players operation #arm tag_union *= #factor_31 tag_union
scoreboard players operation #arm tag_union += #value tag_union
scoreboard players operation #checksum tag_union *= #factor_31 tag_union
scoreboard players operation #checksum tag_union += #arm tag_union
