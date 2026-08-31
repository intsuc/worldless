scoreboard objectives add pred_lower dummy
scoreboard players set #valid pred_lower 1
scoreboard players set #prepared pred_lower 0

data remove storage predicate_lowering:validation request
data modify storage predicate_lowering:validation request set from storage worldless_lab:predicate_lowering/input
data remove storage predicate_lowering:validation request.terms
scoreboard players set #remaining_fields pred_lower -1
execute store result score #remaining_fields pred_lower run data get storage predicate_lowering:validation request
execute unless score #remaining_fields pred_lower matches 0 run scoreboard players set #valid pred_lower 0

function predicate_lowering:validate_array.macro {field:"terms"}
scoreboard players set #term_count pred_lower -1
execute store result score #term_count pred_lower run data get storage worldless_lab:predicate_lowering/input terms
execute unless score #term_count pred_lower matches 16 run scoreboard players set #valid pred_lower 0

scoreboard players set #input0 pred_lower 2
execute store result score #input0 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[0]
execute unless score #input0 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input1 pred_lower 2
execute store result score #input1 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[1]
execute unless score #input1 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input2 pred_lower 2
execute store result score #input2 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[2]
execute unless score #input2 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input3 pred_lower 2
execute store result score #input3 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[3]
execute unless score #input3 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input4 pred_lower 2
execute store result score #input4 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[4]
execute unless score #input4 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input5 pred_lower 2
execute store result score #input5 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[5]
execute unless score #input5 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input6 pred_lower 2
execute store result score #input6 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[6]
execute unless score #input6 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input7 pred_lower 2
execute store result score #input7 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[7]
execute unless score #input7 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input8 pred_lower 2
execute store result score #input8 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[8]
execute unless score #input8 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input9 pred_lower 2
execute store result score #input9 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[9]
execute unless score #input9 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input10 pred_lower 2
execute store result score #input10 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[10]
execute unless score #input10 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input11 pred_lower 2
execute store result score #input11 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[11]
execute unless score #input11 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input12 pred_lower 2
execute store result score #input12 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[12]
execute unless score #input12 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input13 pred_lower 2
execute store result score #input13 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[13]
execute unless score #input13 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input14 pred_lower 2
execute store result score #input14 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[14]
execute unless score #input14 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0
scoreboard players set #input15 pred_lower 2
execute store result score #input15 pred_lower run data get storage worldless_lab:predicate_lowering/input terms[15]
execute unless score #input15 pred_lower matches 0..1 run scoreboard players set #valid pred_lower 0

execute unless score #valid pred_lower matches 1 run return fail

data modify storage predicate_lowering:state work set value {result:{result:0,checksum:0}}
scoreboard players operation #t0 pred_lower = #input0 pred_lower
scoreboard players operation #t1 pred_lower = #input1 pred_lower
scoreboard players operation #t2 pred_lower = #input2 pred_lower
scoreboard players operation #t3 pred_lower = #input3 pred_lower
scoreboard players operation #t4 pred_lower = #input4 pred_lower
scoreboard players operation #t5 pred_lower = #input5 pred_lower
scoreboard players operation #t6 pred_lower = #input6 pred_lower
scoreboard players operation #t7 pred_lower = #input7 pred_lower
scoreboard players operation #t8 pred_lower = #input8 pred_lower
scoreboard players operation #t9 pred_lower = #input9 pred_lower
scoreboard players operation #t10 pred_lower = #input10 pred_lower
scoreboard players operation #t11 pred_lower = #input11 pred_lower
scoreboard players operation #t12 pred_lower = #input12 pred_lower
scoreboard players operation #t13 pred_lower = #input13 pred_lower
scoreboard players operation #t14 pred_lower = #input14 pred_lower
scoreboard players operation #t15 pred_lower = #input15 pred_lower
scoreboard players set #result pred_lower 0
scoreboard players set #checksum pred_lower 1
scoreboard players set #factor_31 pred_lower 31
scoreboard players set #evaluations pred_lower 0
scoreboard players set #evaluation_target pred_lower 63
scoreboard players set #prepared pred_lower 1
return 1
