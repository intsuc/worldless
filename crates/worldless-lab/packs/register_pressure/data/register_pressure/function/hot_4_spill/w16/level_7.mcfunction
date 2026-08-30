scoreboard players operation #l0_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l0_7 reg_pressure 1
scoreboard players operation #l1_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l1_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l1_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l1_7 reg_pressure 2
scoreboard players operation #l2_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l2_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l2_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l2_7 reg_pressure 3
scoreboard players operation #l3_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l3_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l3_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l3_7 reg_pressure 4
scoreboard players operation #l4 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l4 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l4 reg_pressure += #remaining reg_pressure
scoreboard players add #l4 reg_pressure 5
scoreboard players operation #l5 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l5 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l5 reg_pressure += #remaining reg_pressure
scoreboard players add #l5 reg_pressure 6
scoreboard players operation #l6 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l6 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l6 reg_pressure += #remaining reg_pressure
scoreboard players add #l6 reg_pressure 7
scoreboard players operation #l7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l7 reg_pressure += #remaining reg_pressure
scoreboard players add #l7 reg_pressure 8
scoreboard players operation #l8 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l8 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l8 reg_pressure += #remaining reg_pressure
scoreboard players add #l8 reg_pressure 9
scoreboard players operation #l9 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l9 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l9 reg_pressure += #remaining reg_pressure
scoreboard players add #l9 reg_pressure 10
scoreboard players operation #l10 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l10 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l10 reg_pressure += #remaining reg_pressure
scoreboard players add #l10 reg_pressure 11
scoreboard players operation #l11 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l11 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l11 reg_pressure += #remaining reg_pressure
scoreboard players add #l11 reg_pressure 12
scoreboard players operation #l12 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l12 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l12 reg_pressure += #remaining reg_pressure
scoreboard players add #l12 reg_pressure 13
scoreboard players operation #l13 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l13 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l13 reg_pressure += #remaining reg_pressure
scoreboard players add #l13 reg_pressure 14
scoreboard players operation #l14 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l14 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l14 reg_pressure += #remaining reg_pressure
scoreboard players add #l14 reg_pressure 15
scoreboard players operation #l15 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l15 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l15 reg_pressure += #remaining reg_pressure
scoreboard players add #l15 reg_pressure 16
scoreboard players add #activations reg_pressure 1
execute if score #remaining reg_pressure matches 1 run return run function register_pressure:hot_4_spill/w16/base_7
return fail
