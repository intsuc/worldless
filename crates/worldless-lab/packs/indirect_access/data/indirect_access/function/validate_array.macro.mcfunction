data remove storage indirect_access:validation array_probe
$data modify storage indirect_access:validation array_probe set from storage worldless_lab:indirect_access/input $(field)
data remove storage indirect_access:validation array_probe[]
execute unless data storage indirect_access:validation {array_probe:[I;]} run scoreboard players set #valid indirect_access 0
