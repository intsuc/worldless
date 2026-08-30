scoreboard players set #word_count call_frames -1
execute store result score #word_count call_frames run data get storage call_frames:state work.words
execute unless score #word_count call_frames matches 0 run return fail
scoreboard players set #frame_count call_frames -1
execute store result score #frame_count call_frames run data get storage call_frames:state work.frames
execute unless score #frame_count call_frames matches 0 run return fail

execute store result storage call_frames:state work.result.checksum int 1 run scoreboard players get #checksum call_frames
data remove storage worldless_lab:call_frames/output checksum
data modify storage worldless_lab:call_frames/output checksum set from storage call_frames:state work.result.checksum
return run execute if data storage worldless_lab:call_frames/output checksum
