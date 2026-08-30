scoreboard players operation #left_10 call_frames = #arg_x call_frames
scoreboard players operation #left_10 call_frames *= #factor_17 call_frames
scoreboard players operation #left_10 call_frames += #remaining call_frames
scoreboard players operation #right_10 call_frames = #arg_x call_frames
scoreboard players operation #right_10 call_frames *= #factor_31 call_frames
scoreboard players operation #right_10 call_frames -= #remaining call_frames
execute if score #remaining call_frames matches 1 run return run function call_frames:static_scores/base_10

scoreboard players operation #arg_x call_frames += #left_10 call_frames
scoreboard players remove #remaining call_frames 1
execute store result score #result call_frames run function call_frames:static_scores/level_11
scoreboard players operation #selected call_frames = #right_10 call_frames
execute if score #result call_frames matches ..-1 run scoreboard players operation #selected call_frames = #left_10 call_frames
scoreboard players operation #result call_frames *= #factor_31 call_frames
scoreboard players operation #result call_frames += #selected call_frames
return run scoreboard players get #result call_frames
