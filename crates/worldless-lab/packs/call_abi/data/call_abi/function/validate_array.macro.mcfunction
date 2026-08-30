data remove storage call_abi:validation array_probe
$data modify storage call_abi:validation array_probe set from storage worldless_lab:call_abi/input $(field)
data remove storage call_abi:validation array_probe[]
execute unless data storage call_abi:validation {array_probe:[I;]} run scoreboard players set #valid call_abi 0
