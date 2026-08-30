scoreboard players operation #left_15 call_frames = #arg_x call_frames
scoreboard players operation #left_15 call_frames *= #factor_17 call_frames
scoreboard players operation #left_15 call_frames += #remaining call_frames
scoreboard players operation #right_15 call_frames = #arg_x call_frames
scoreboard players operation #right_15 call_frames *= #factor_31 call_frames
scoreboard players operation #right_15 call_frames -= #remaining call_frames
execute if score #remaining call_frames matches 1 run return run function call_frames:static_scores/base_15

return fail
