scoreboard players operation #l0 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0 reg_pressure += #remaining reg_pressure
scoreboard players add #l0 reg_pressure 1
scoreboard players add #activations reg_pressure 1
execute if score #remaining reg_pressure matches 1 run return run function register_pressure:dynamic_base/w1

data modify storage register_pressure:state work.frames append value {l0:0}
execute store result storage register_pressure:state work.frames[-1].l0 int 1 run scoreboard players get #l0 reg_pressure
scoreboard players operation #arg_x reg_pressure += #l0 reg_pressure
scoreboard players remove #remaining reg_pressure 1
execute store result score #result reg_pressure run function register_pressure:compound_stack/w1/recur
execute store result score #l0 reg_pressure run data get storage register_pressure:state work.frames[-1].l0
data remove storage register_pressure:state work.frames[-1]

scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l0 reg_pressure
scoreboard players add #folds reg_pressure 1
return run scoreboard players get #result reg_pressure
