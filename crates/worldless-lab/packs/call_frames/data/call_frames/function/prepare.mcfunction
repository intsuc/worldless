scoreboard objectives add call_frames dummy
scoreboard players set #valid call_frames 1

data remove storage call_frames:validation request
data modify storage call_frames:validation request set from storage worldless_lab:call_frames/input
data remove storage call_frames:validation request.depth
data remove storage call_frames:validation request.seeds
scoreboard players set #remaining_fields call_frames -1
execute store result score #remaining_fields call_frames run data get storage call_frames:validation request
execute unless score #remaining_fields call_frames matches 0 run scoreboard players set #valid call_frames 0

function call_frames:validate_array.macro {field:"seeds"}
scoreboard players set #seed_length call_frames -1
execute store result score #seed_length call_frames run data get storage worldless_lab:call_frames/input seeds
execute unless score #seed_length call_frames matches 31 run scoreboard players set #valid call_frames 0

scoreboard players set #depth call_frames 0
execute store result score #depth call_frames run data get storage worldless_lab:call_frames/input depth
execute unless score #depth call_frames matches 1..16 run scoreboard players set #valid call_frames 0
data modify storage call_frames:validation macro set value {depth:0}
execute store result storage call_frames:validation macro.depth int 1 run scoreboard players get #depth call_frames
function call_frames:validate_depth.macro with storage call_frames:validation macro

execute unless score #valid call_frames matches 1 run return fail

data modify storage call_frames:state work set value {words:[I;],frames:[],result:{checksum:0}}
scoreboard players set #checksum call_frames 1
scoreboard players set #factor_17 call_frames 17
scoreboard players set #factor_31 call_frames 31
return 1
