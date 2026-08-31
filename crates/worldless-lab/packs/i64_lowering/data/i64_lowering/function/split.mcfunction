data modify storage i64_lowering:validation wrapper[0] set from storage i64_lowering:validation source
execute store result score #split_low i64_lowering run data get storage i64_lowering:validation wrapper[0] 1

execute store result score #split_high i64_lowering run data get storage i64_lowering:validation source 0.00000000023283064365386963
execute store result score #complement i64_lowering run data get storage i64_lowering:validation source -0.00000000023283064365386963

scoreboard players operation #correction i64_lowering = #split_high i64_lowering
scoreboard players operation #correction i64_lowering > #complement i64_lowering

execute if score #split_low i64_lowering matches 0.. run return 0
execute if score #correction i64_lowering matches 0..2097151 run return 0
execute if score #split_low i64_lowering matches -512.. if score #correction i64_lowering matches 1073741824..2147483646 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -256.. if score #correction i64_lowering matches 536870912..1073741823 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -128.. if score #correction i64_lowering matches 268435456..536870911 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -64.. if score #correction i64_lowering matches 134217728..268435455 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -32.. if score #correction i64_lowering matches 67108864..134217727 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -16.. if score #correction i64_lowering matches 33554432..67108863 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -8.. if score #correction i64_lowering matches 16777216..33554431 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -4.. if score #correction i64_lowering matches 8388608..16777215 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -2.. if score #correction i64_lowering matches 4194304..8388607 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #split_low i64_lowering matches -1.. if score #correction i64_lowering matches 2097152..4194303 run return run scoreboard players remove #split_high i64_lowering 1
execute if score #correction i64_lowering matches 2147483647.. if score #split_low i64_lowering matches -512.. if score #complement i64_lowering matches -2147483647.. run scoreboard players remove #split_high i64_lowering 1
