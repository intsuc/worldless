data remove storage call_frames:validation array_probe
$data modify storage call_frames:validation array_probe set from storage worldless_lab:call_frames/input $(field)
data remove storage call_frames:validation array_probe[]
execute unless data storage call_frames:validation {array_probe:[I;]} run scoreboard players set #valid call_frames 0
