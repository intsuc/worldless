execute unless score #evaluations tag_union = #tag_count tag_union run return fail

data modify storage tagged_union_layout:state work.result set value {checksum:0}
execute store result storage tagged_union_layout:state work.result.checksum int 1 run scoreboard players get #checksum tag_union
execute unless data storage tagged_union_layout:state work.result.checksum run return fail
data modify storage worldless_lab:tagged_union_layout/output checksum set from storage tagged_union_layout:state work.result.checksum
return run execute if data storage worldless_lab:tagged_union_layout/output checksum
