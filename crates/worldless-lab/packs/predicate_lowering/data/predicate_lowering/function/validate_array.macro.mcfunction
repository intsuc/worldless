data remove storage predicate_lowering:validation array_probe
$data modify storage predicate_lowering:validation array_probe set from storage worldless_lab:predicate_lowering/input $(field)
data remove storage predicate_lowering:validation array_probe[]
execute unless data storage predicate_lowering:validation {array_probe:[I;]} run scoreboard players set #valid pred_lower 0
