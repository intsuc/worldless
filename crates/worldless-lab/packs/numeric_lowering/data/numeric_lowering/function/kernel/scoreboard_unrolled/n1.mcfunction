execute store result score #dot numeric_lowering run data get storage worldless_lab:numeric_lowering/input a[0]
execute store result score #right numeric_lowering run data get storage worldless_lab:numeric_lowering/input b[0]
scoreboard players operation #dot numeric_lowering *= #right numeric_lowering
