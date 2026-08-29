scoreboard players set #request_valid transformer 1
data modify storage transformer:validation request set from storage transformer:request
data remove storage transformer:validation request.prefix
data remove storage transformer:validation request.max_new_tokens
scoreboard players set #actual transformer -1
execute store result score #actual transformer run data get storage transformer:validation request
execute unless score #actual transformer matches 0 run scoreboard players set #request_valid transformer 0

data remove storage transformer:validation string_probe
scoreboard players set #type_valid transformer 0
execute store success score #type_valid transformer run data modify storage transformer:validation string_probe set string storage transformer:request prefix
execute unless score #type_valid transformer matches 1 run scoreboard players set #request_valid transformer 0
data modify storage transformer:validation numeric_probe set value [I;]
scoreboard players set #numeric_type transformer 0
execute store success score #numeric_type transformer run data modify storage transformer:validation numeric_probe append from storage transformer:request prefix
execute if score #numeric_type transformer matches 1 run scoreboard players set #request_valid transformer 0

scoreboard players set #max_new transformer -1
execute store result score #max_new transformer run data get storage transformer:request max_new_tokens
execute unless score #max_new transformer matches 1..256 run scoreboard players set #request_valid transformer 0
execute store result storage transformer:validation macro.max_new_tokens int 1 run scoreboard players get #max_new transformer
function transformer:infer/validate_max_new_type.macro with storage transformer:validation macro
return run scoreboard players get #request_valid transformer
