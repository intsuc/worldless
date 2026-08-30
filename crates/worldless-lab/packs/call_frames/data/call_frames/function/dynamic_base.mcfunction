scoreboard players operation #result call_frames = #left call_frames
scoreboard players operation #result call_frames *= #factor_31 call_frames
scoreboard players operation #result call_frames += #right call_frames
scoreboard players add #result call_frames 7
return run scoreboard players get #result call_frames
