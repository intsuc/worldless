scoreboard players operation #l0_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l0_3 reg_pressure 1
scoreboard players operation #l1_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l1_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l1_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l1_3 reg_pressure 2
scoreboard players operation #l2_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l2_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l2_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l2_3 reg_pressure 3
scoreboard players operation #l3_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l3_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l3_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l3_3 reg_pressure 4
scoreboard players operation #l4_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l4_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l4_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l4_3 reg_pressure 5
scoreboard players operation #l5_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l5_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l5_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l5_3 reg_pressure 6
scoreboard players operation #l6_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l6_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l6_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l6_3 reg_pressure 7
scoreboard players operation #l7_3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l7_3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l7_3 reg_pressure += #remaining reg_pressure
scoreboard players add #l7_3 reg_pressure 8
scoreboard players add #activations reg_pressure 1
execute if score #remaining reg_pressure matches 1 run return fail

scoreboard players operation #arg_x reg_pressure += #l0_3 reg_pressure
scoreboard players remove #remaining reg_pressure 1
execute store result score #result reg_pressure run function register_pressure:static_scores/w8/level_4

scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l0_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l1_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l2_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l3_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l4_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l5_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l6_3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l7_3 reg_pressure
scoreboard players add #folds reg_pressure 1
return run scoreboard players get #result reg_pressure
