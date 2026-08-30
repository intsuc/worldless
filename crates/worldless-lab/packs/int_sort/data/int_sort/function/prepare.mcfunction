scoreboard objectives add int_sort dummy
scoreboard players set #valid int_sort 1

data remove storage int_sort:validation request
data modify storage int_sort:validation request set from storage worldless_lab:int_sort/input
data remove storage int_sort:validation request.values
scoreboard players set #remaining int_sort -1
execute store result score #remaining int_sort run data get storage int_sort:validation request
execute unless score #remaining int_sort matches 0 run scoreboard players set #valid int_sort 0

data remove storage int_sort:validation array_probe
data modify storage int_sort:validation array_probe set from storage worldless_lab:int_sort/input values
data remove storage int_sort:validation array_probe[]
execute unless data storage int_sort:validation {array_probe:[I;]} run scoreboard players set #valid int_sort 0
execute unless score #valid int_sort matches 1 run return 0

data modify storage int_sort:state work set value {values:[I;],next:[I;],macro:{}}
data modify storage int_sort:state work.values set from storage worldless_lab:int_sort/input values
execute store result score #length int_sort run data get storage int_sort:state work.values
return 1
