scoreboard players operation #result reg_pressure = #arg_x reg_pressure
scoreboard players operation #result reg_pressure *= #factor_17 reg_pressure
scoreboard players add #result reg_pressure 7
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l0 reg_pressure
scoreboard players add #folds reg_pressure 1
return run scoreboard players get #result reg_pressure
