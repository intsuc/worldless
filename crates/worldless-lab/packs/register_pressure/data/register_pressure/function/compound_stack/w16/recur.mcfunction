scoreboard players operation #l0 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l0 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l0 reg_pressure += #remaining reg_pressure
scoreboard players add #l0 reg_pressure 1
scoreboard players operation #l1 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l1 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l1 reg_pressure += #remaining reg_pressure
scoreboard players add #l1 reg_pressure 2
scoreboard players operation #l2 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l2 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l2 reg_pressure += #remaining reg_pressure
scoreboard players add #l2 reg_pressure 3
scoreboard players operation #l3 reg_pressure = #arg_x reg_pressure
scoreboard players operation #l3 reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #l3 reg_pressure += #remaining reg_pressure
scoreboard players add #l3 reg_pressure 4
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
execute if score #remaining reg_pressure matches 1 run return run function register_pressure:dynamic_base/w16

data modify storage register_pressure:state work.frames append value {l0:0,l1:0,l2:0,l3:0,l4:0,l5:0,l6:0,l7:0,l8:0,l9:0,l10:0,l11:0,l12:0,l13:0,l14:0,l15:0}
execute store result storage register_pressure:state work.frames[-1].l0 int 1 run scoreboard players get #l0 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l1 int 1 run scoreboard players get #l1 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l2 int 1 run scoreboard players get #l2 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l3 int 1 run scoreboard players get #l3 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l4 int 1 run scoreboard players get #l4 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l5 int 1 run scoreboard players get #l5 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l6 int 1 run scoreboard players get #l6 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l7 int 1 run scoreboard players get #l7 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l8 int 1 run scoreboard players get #l8 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l9 int 1 run scoreboard players get #l9 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l10 int 1 run scoreboard players get #l10 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l11 int 1 run scoreboard players get #l11 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l12 int 1 run scoreboard players get #l12 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l13 int 1 run scoreboard players get #l13 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l14 int 1 run scoreboard players get #l14 reg_pressure
execute store result storage register_pressure:state work.frames[-1].l15 int 1 run scoreboard players get #l15 reg_pressure
scoreboard players operation #arg_x reg_pressure += #l0 reg_pressure
scoreboard players remove #remaining reg_pressure 1
execute store result score #result reg_pressure run function register_pressure:compound_stack/w16/recur
execute store result score #l0 reg_pressure run data get storage register_pressure:state work.frames[-1].l0
execute store result score #l1 reg_pressure run data get storage register_pressure:state work.frames[-1].l1
execute store result score #l2 reg_pressure run data get storage register_pressure:state work.frames[-1].l2
execute store result score #l3 reg_pressure run data get storage register_pressure:state work.frames[-1].l3
execute store result score #l4 reg_pressure run data get storage register_pressure:state work.frames[-1].l4
execute store result score #l5 reg_pressure run data get storage register_pressure:state work.frames[-1].l5
execute store result score #l6 reg_pressure run data get storage register_pressure:state work.frames[-1].l6
execute store result score #l7 reg_pressure run data get storage register_pressure:state work.frames[-1].l7
execute store result score #l8 reg_pressure run data get storage register_pressure:state work.frames[-1].l8
execute store result score #l9 reg_pressure run data get storage register_pressure:state work.frames[-1].l9
execute store result score #l10 reg_pressure run data get storage register_pressure:state work.frames[-1].l10
execute store result score #l11 reg_pressure run data get storage register_pressure:state work.frames[-1].l11
execute store result score #l12 reg_pressure run data get storage register_pressure:state work.frames[-1].l12
execute store result score #l13 reg_pressure run data get storage register_pressure:state work.frames[-1].l13
execute store result score #l14 reg_pressure run data get storage register_pressure:state work.frames[-1].l14
execute store result score #l15 reg_pressure run data get storage register_pressure:state work.frames[-1].l15
data remove storage register_pressure:state work.frames[-1]

scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l0 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l1 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l2 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l3 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l4 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l5 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l6 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l7 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l8 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l9 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l10 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l11 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l12 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l13 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l14 reg_pressure
scoreboard players add #folds reg_pressure 1
scoreboard players operation #result reg_pressure *= #factor_31 reg_pressure
scoreboard players operation #result reg_pressure += #l15 reg_pressure
scoreboard players add #folds reg_pressure 1
return run scoreboard players get #result reg_pressure
