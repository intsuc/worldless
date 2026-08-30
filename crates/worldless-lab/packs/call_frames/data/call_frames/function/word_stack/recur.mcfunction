scoreboard players operation #left call_frames = #arg_x call_frames
scoreboard players operation #left call_frames *= #factor_17 call_frames
scoreboard players operation #left call_frames += #remaining call_frames
scoreboard players operation #right call_frames = #arg_x call_frames
scoreboard players operation #right call_frames *= #factor_31 call_frames
scoreboard players operation #right call_frames -= #remaining call_frames
execute if score #remaining call_frames matches 1 run return run function call_frames:dynamic_base

data modify storage call_frames:state work.words append value 0
execute store result storage call_frames:state work.words[-1] int 1 run scoreboard players get #left call_frames
data modify storage call_frames:state work.words append value 0
execute store result storage call_frames:state work.words[-1] int 1 run scoreboard players get #right call_frames
scoreboard players operation #arg_x call_frames += #left call_frames
scoreboard players remove #remaining call_frames 1
execute store result score #result call_frames run function call_frames:word_stack/recur
execute store result score #right call_frames run data get storage call_frames:state work.words[-1]
data remove storage call_frames:state work.words[-1]
execute store result score #left call_frames run data get storage call_frames:state work.words[-1]
data remove storage call_frames:state work.words[-1]

scoreboard players operation #selected call_frames = #right call_frames
execute if score #result call_frames matches ..-1 run scoreboard players operation #selected call_frames = #left call_frames
scoreboard players operation #result call_frames *= #factor_31 call_frames
scoreboard players operation #result call_frames += #selected call_frames
return run scoreboard players get #result call_frames
