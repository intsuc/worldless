scoreboard objectives add reg_pressure dummy
scoreboard players set #valid reg_pressure 1

data remove storage register_pressure:validation request
data modify storage register_pressure:validation request set from storage worldless_lab:register_pressure/input
data remove storage register_pressure:validation request.width
data remove storage register_pressure:validation request.seeds
scoreboard players set #remaining_fields reg_pressure -1
execute store result score #remaining_fields reg_pressure run data get storage register_pressure:validation request
execute unless score #remaining_fields reg_pressure matches 0 run scoreboard players set #valid reg_pressure 0

function register_pressure:validate_array.macro {field:"seeds"}
scoreboard players set #seed_length reg_pressure -1
execute store result score #seed_length reg_pressure run data get storage worldless_lab:register_pressure/input seeds
execute unless score #seed_length reg_pressure matches 15 run scoreboard players set #valid reg_pressure 0

scoreboard players set #input_width reg_pressure 0
execute store result score #input_width reg_pressure run data get storage worldless_lab:register_pressure/input width
data modify storage register_pressure:validation macro set value {width:0}
execute store result storage register_pressure:validation macro.width int 1 run scoreboard players get #input_width reg_pressure
function register_pressure:validate_width.macro with storage register_pressure:validation macro
scoreboard players set #allowed_width reg_pressure 0
execute if score #input_width reg_pressure matches 1 run scoreboard players set #allowed_width reg_pressure 1
execute if score #input_width reg_pressure matches 2 run scoreboard players set #allowed_width reg_pressure 1
execute if score #input_width reg_pressure matches 4 run scoreboard players set #allowed_width reg_pressure 1
execute if score #input_width reg_pressure matches 8 run scoreboard players set #allowed_width reg_pressure 1
execute if score #input_width reg_pressure matches 16 run scoreboard players set #allowed_width reg_pressure 1
execute unless score #allowed_width reg_pressure matches 1 run scoreboard players set #valid reg_pressure 0
execute unless score #valid reg_pressure matches 1 run return fail

data modify storage register_pressure:state work set value {words:[I;],frames:[],overflow:[I;],macro:{width:0,variant:""},result:{width:0,checksum:0}}
execute store result storage register_pressure:state work.macro.width int 1 run scoreboard players get #input_width reg_pressure
scoreboard players operation #requested_width reg_pressure = #input_width reg_pressure
scoreboard players set #checksum reg_pressure 1
scoreboard players set #factor_17 reg_pressure 17
scoreboard players set #factor_31 reg_pressure 31
scoreboard players set #roots reg_pressure 0
scoreboard players set #activations reg_pressure 0
scoreboard players set #folds reg_pressure 0
scoreboard players set #activation_target reg_pressure 120
return 1
