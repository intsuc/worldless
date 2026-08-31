scoreboard objectives add scalar_replace dummy
scoreboard players set #valid scalar_replace 1

data remove storage scalar_replacement:validation request
data modify storage scalar_replacement:validation request set from storage worldless_lab:scalar_replacement/input
data remove storage scalar_replacement:validation request.width
data remove storage scalar_replacement:validation request.rounds
data remove storage scalar_replacement:validation request.seed
scoreboard players set #remaining_fields scalar_replace -1
execute store result score #remaining_fields scalar_replace run data get storage scalar_replacement:validation request
execute unless score #remaining_fields scalar_replace matches 0 run scoreboard players set #valid scalar_replace 0

scoreboard players set #width scalar_replace 0
scoreboard players set #rounds scalar_replace 0
scoreboard players set #seed scalar_replace 0
execute store result score #width scalar_replace run data get storage worldless_lab:scalar_replacement/input width
execute store result score #rounds scalar_replace run data get storage worldless_lab:scalar_replacement/input rounds
execute store result score #seed scalar_replace run data get storage worldless_lab:scalar_replacement/input seed
data modify storage scalar_replacement:validation scalars set value {width:0,rounds:0,seed:0}
execute store result storage scalar_replacement:validation scalars.width int 1 run scoreboard players get #width scalar_replace
execute store result storage scalar_replacement:validation scalars.rounds int 1 run scoreboard players get #rounds scalar_replace
execute store result storage scalar_replacement:validation scalars.seed int 1 run scoreboard players get #seed scalar_replace
function scalar_replacement:validate_scalars.macro with storage scalar_replacement:validation scalars

scoreboard players set #allowed_width scalar_replace 0
execute if score #width scalar_replace matches 1 run scoreboard players set #allowed_width scalar_replace 1
execute if score #width scalar_replace matches 4 run scoreboard players set #allowed_width scalar_replace 1
execute if score #width scalar_replace matches 8 run scoreboard players set #allowed_width scalar_replace 1
execute if score #width scalar_replace matches 16 run scoreboard players set #allowed_width scalar_replace 1
execute unless score #allowed_width scalar_replace matches 1 run scoreboard players set #valid scalar_replace 0

scoreboard players set #allowed_rounds scalar_replace 0
execute if score #rounds scalar_replace matches 1 run scoreboard players set #allowed_rounds scalar_replace 1
execute if score #rounds scalar_replace matches 4 run scoreboard players set #allowed_rounds scalar_replace 1
execute if score #rounds scalar_replace matches 16 run scoreboard players set #allowed_rounds scalar_replace 1
execute unless score #allowed_rounds scalar_replace matches 1 run scoreboard players set #valid scalar_replace 0

execute unless score #valid scalar_replace matches 1 run return fail
data modify storage scalar_replacement:state macro set value {width:0,rounds:0,variant:""}
execute store result storage scalar_replacement:state macro.width int 1 run scoreboard players get #width scalar_replace
execute store result storage scalar_replacement:state macro.rounds int 1 run scoreboard players get #rounds scalar_replace
scoreboard players set #lcg_factor scalar_replace 1664525
scoreboard players set #factor scalar_replace 31
return 1
