$execute store success score #numeric i64_lowering run data get storage worldless_lab:i64_lowering/input $(field) 0
data remove storage i64_lowering:validation suffix
$execute if score #numeric i64_lowering matches 1 run data modify storage i64_lowering:validation suffix set string storage worldless_lab:i64_lowering/input $(field) -1
execute unless data storage i64_lowering:validation {suffix:"L"} run scoreboard players set #valid i64_lowering 0
