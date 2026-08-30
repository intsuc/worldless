data remove storage numeric_lowering:validation array_probe
$data modify storage numeric_lowering:validation array_probe set from storage worldless_lab:numeric_lowering/input $(field)
data remove storage numeric_lowering:validation array_probe[]
execute unless data storage numeric_lowering:validation {array_probe:[I;]} run scoreboard players set #valid numeric_lowering 0
