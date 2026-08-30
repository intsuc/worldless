execute store result score #minimum numeric_lowering run compute default {"type":"minimum","operands":[{"type":"storage","storage":"worldless_lab:numeric_lowering/input","path":"a[0]"},{"type":"storage","storage":"worldless_lab:numeric_lowering/input","path":"b[0]"}]} integer
execute store result score #maximum numeric_lowering run compute default {"type":"maximum","operands":[{"type":"storage","storage":"worldless_lab:numeric_lowering/input","path":"a[0]"},{"type":"storage","storage":"worldless_lab:numeric_lowering/input","path":"b[0]"}]} integer
execute unless score #minimum numeric_lowering matches -128.. run scoreboard players set #valid numeric_lowering 0
execute unless score #maximum numeric_lowering matches ..127 run scoreboard players set #valid numeric_lowering 0
