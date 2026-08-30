scoreboard players operation #l0_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l0_7 reg_pressure 1
scoreboard players operation #l1_7 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l1_7 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l1_7 reg_pressure += #remaining reg_pressure
scoreboard players add #l1_7 reg_pressure 2
scoreboard players add #activations reg_pressure 1
execute if score #remaining reg_pressure matches 1 run return run function register_pressure:hot_4_spill/w2/base_7
return fail
