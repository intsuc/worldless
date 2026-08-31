scoreboard objectives add tag_union dummy
scoreboard players set #valid tag_union 1
scoreboard players set #prepared tag_union 0

data remove storage tagged_union_layout:validation request
data modify storage tagged_union_layout:validation request set from storage worldless_lab:tagged_union_layout/input
data remove storage tagged_union_layout:validation request.tags
data remove storage tagged_union_layout:validation request.seed
scoreboard players set #remaining_fields tag_union -1
execute store result score #remaining_fields tag_union run data get storage tagged_union_layout:validation request
execute unless score #remaining_fields tag_union matches 0 run scoreboard players set #valid tag_union 0

function tagged_union_layout:validate_array.macro {field:"tags"}
scoreboard players set #tag_count tag_union -1
execute store result score #tag_count tag_union run data get storage worldless_lab:tagged_union_layout/input tags
execute unless score #tag_count tag_union matches 31 run scoreboard players set #valid tag_union 0

scoreboard players set #input_seed tag_union 0
execute store result score #input_seed tag_union run data get storage worldless_lab:tagged_union_layout/input seed
data modify storage tagged_union_layout:validation scalars set value {seed:0}
execute store result storage tagged_union_layout:validation scalars.seed int 1 run scoreboard players get #input_seed tag_union
function tagged_union_layout:validate_seed.macro with storage tagged_union_layout:validation scalars

scoreboard players set #input0 tag_union -1
execute store result score #input0 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[0]
execute unless score #input0 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input1 tag_union -1
execute store result score #input1 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[1]
execute unless score #input1 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input2 tag_union -1
execute store result score #input2 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[2]
execute unless score #input2 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input3 tag_union -1
execute store result score #input3 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[3]
execute unless score #input3 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input4 tag_union -1
execute store result score #input4 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[4]
execute unless score #input4 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input5 tag_union -1
execute store result score #input5 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[5]
execute unless score #input5 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input6 tag_union -1
execute store result score #input6 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[6]
execute unless score #input6 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input7 tag_union -1
execute store result score #input7 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[7]
execute unless score #input7 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input8 tag_union -1
execute store result score #input8 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[8]
execute unless score #input8 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input9 tag_union -1
execute store result score #input9 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[9]
execute unless score #input9 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input10 tag_union -1
execute store result score #input10 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[10]
execute unless score #input10 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input11 tag_union -1
execute store result score #input11 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[11]
execute unless score #input11 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input12 tag_union -1
execute store result score #input12 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[12]
execute unless score #input12 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input13 tag_union -1
execute store result score #input13 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[13]
execute unless score #input13 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input14 tag_union -1
execute store result score #input14 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[14]
execute unless score #input14 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input15 tag_union -1
execute store result score #input15 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[15]
execute unless score #input15 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input16 tag_union -1
execute store result score #input16 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[16]
execute unless score #input16 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input17 tag_union -1
execute store result score #input17 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[17]
execute unless score #input17 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input18 tag_union -1
execute store result score #input18 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[18]
execute unless score #input18 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input19 tag_union -1
execute store result score #input19 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[19]
execute unless score #input19 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input20 tag_union -1
execute store result score #input20 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[20]
execute unless score #input20 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input21 tag_union -1
execute store result score #input21 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[21]
execute unless score #input21 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input22 tag_union -1
execute store result score #input22 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[22]
execute unless score #input22 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input23 tag_union -1
execute store result score #input23 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[23]
execute unless score #input23 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input24 tag_union -1
execute store result score #input24 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[24]
execute unless score #input24 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input25 tag_union -1
execute store result score #input25 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[25]
execute unless score #input25 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input26 tag_union -1
execute store result score #input26 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[26]
execute unless score #input26 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input27 tag_union -1
execute store result score #input27 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[27]
execute unless score #input27 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input28 tag_union -1
execute store result score #input28 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[28]
execute unless score #input28 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input29 tag_union -1
execute store result score #input29 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[29]
execute unless score #input29 tag_union matches 0..3 run scoreboard players set #valid tag_union 0
scoreboard players set #input30 tag_union -1
execute store result score #input30 tag_union run data get storage worldless_lab:tagged_union_layout/input tags[30]
execute unless score #input30 tag_union matches 0..3 run scoreboard players set #valid tag_union 0

execute unless score #valid tag_union matches 1 run return fail

data modify storage tagged_union_layout:state work set value {cell:{},result:{checksum:0}}
scoreboard players operation #state tag_union = #input_seed tag_union
scoreboard players set #checksum tag_union 1
scoreboard players set #lcg_multiplier tag_union 1664525
scoreboard players set #lcg_addend tag_union 1013904223
scoreboard players set #factor_31 tag_union 31
scoreboard players set #evaluations tag_union 0
scoreboard players set #prepared tag_union 1
return 1
