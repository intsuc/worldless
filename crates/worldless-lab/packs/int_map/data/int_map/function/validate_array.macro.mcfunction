data remove storage int_map:validation array_probe
$data modify storage int_map:validation array_probe set from storage worldless_lab:int_map/input $(field)
data remove storage int_map:validation array_probe[]
execute unless data storage int_map:validation {array_probe:[I;]} run scoreboard players set #valid int_map 0
