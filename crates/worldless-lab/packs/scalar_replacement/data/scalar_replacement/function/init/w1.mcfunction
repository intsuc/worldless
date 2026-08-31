data modify storage scalar_replacement:state work set value {values:[I;0],result:{}}
scoreboard players operation #generator scalar_replace = #seed scalar_replace
execute store result storage scalar_replacement:state work.values[0] int 1 run scoreboard players get #generator scalar_replace
scoreboard players operation #generator scalar_replace *= #lcg_factor scalar_replace
scoreboard players add #generator scalar_replace 1013904223
