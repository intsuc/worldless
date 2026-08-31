data remove storage tagged_union_layout:validation array_probe
$data modify storage tagged_union_layout:validation array_probe set from storage worldless_lab:tagged_union_layout/input $(field)
data remove storage tagged_union_layout:validation array_probe[]
execute unless data storage tagged_union_layout:validation {array_probe:[I;]} run scoreboard players set #valid tag_union 0
