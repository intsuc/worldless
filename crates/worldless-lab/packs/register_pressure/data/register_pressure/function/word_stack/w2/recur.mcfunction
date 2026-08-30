scoreboard players operation #l0 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0 reg_pressure += #remaining reg_pressure
scoreboard players add #l0 reg_pressure 1
scoreboard players operation #l1 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l1 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l1 reg_pressure += #remaining reg_pressure
scoreboard players add #l1 reg_pressure 2
scoreboard players add #activations reg_pressure 1
execute if score #remaining reg_pressure matches 1 run return run function register_pressure:dynamic_base/w2

data modify storage register_pressure:state work.words append value 0
execute store result storage register_pressure:state work.words[-1] int 1 run scoreboard players get #l0 reg_pressure
data modify storage register_pressure:state work.words append value 0
execute store result storage register_pressure:state work.words[-1] int 1 run scoreboard players get #l1 reg_pressure
scoreboard players operation #arg_x reg_pressure += #l0 reg_pressure
scoreboard players remove #remaining reg_pressure 1
execute store result score #result reg_pressure run function register_pressure:word_stack/w2/recur
execute store result score #l1 reg_pressure run data get storage register_pressure:state work.words[-1]
data remove storage register_pressure:state work.words[-1]
execute store result score #l0 reg_pressure run data get storage register_pressure:state work.words[-1]
data remove storage register_pressure:state work.words[-1]

scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l0 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l1 reg_pressure
scoreboard players add #folds reg_pressure 1
return run scoreboard players get #result reg_pressure
