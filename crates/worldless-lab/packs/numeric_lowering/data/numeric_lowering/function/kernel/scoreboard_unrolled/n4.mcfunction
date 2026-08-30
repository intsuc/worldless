execute store result score #dot numeric_lowering run data get storage worldless_lab:numeric_lowering/input a[0]
execute store result score #right numeric_lowering run data get storage worldless_lab:numeric_lowering/input b[0]
scoreboard players operation #dot numeric_lowering *= #right numeric_lowering
execute store result score #term numeric_lowering run data get storage worldless_lab:numeric_lowering/input a[1]
execute store result score #right numeric_lowering run data get storage worldless_lab:numeric_lowering/input b[1]
scoreboard players operation #term numeric_lowering *= #right numeric_lowering
scoreboard players operation #dot numeric_lowering += #term numeric_lowering
execute store result score #term numeric_lowering run data get storage worldless_lab:numeric_lowering/input a[2]
execute store result score #right numeric_lowering run data get storage worldless_lab:numeric_lowering/input b[2]
scoreboard players operation #term numeric_lowering *= #right numeric_lowering
scoreboard players operation #dot numeric_lowering += #term numeric_lowering
execute store result score #term numeric_lowering run data get storage worldless_lab:numeric_lowering/input a[3]
execute store result score #right numeric_lowering run data get storage worldless_lab:numeric_lowering/input b[3]
scoreboard players operation #term numeric_lowering *= #right numeric_lowering
scoreboard players operation #dot numeric_lowering += #term numeric_lowering
