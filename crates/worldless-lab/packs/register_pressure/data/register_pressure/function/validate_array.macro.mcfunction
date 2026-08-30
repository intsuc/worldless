data remove storage register_pressure:validation array_probe
$data modify storage register_pressure:validation array_probe set from storage worldless_lab:register_pressure/input $(field)
data remove storage register_pressure:validation array_probe[]
execute unless data storage register_pressure:validation {array_probe:[I;]} run scoreboard players set #valid reg_pressure 0
