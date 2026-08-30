scoreboard players set #word_count reg_pressure -1
execute store result score #word_count reg_pressure run data get storage register_pressure:state work.words
execute unless score #word_count reg_pressure matches 0 run return fail
scoreboard players set #frame_count reg_pressure -1
execute store result score #frame_count reg_pressure run data get storage register_pressure:state work.frames
execute unless score #frame_count reg_pressure matches 0 run return fail
scoreboard players set #overflow_count reg_pressure -1
execute store result score #overflow_count reg_pressure run data get storage register_pressure:state work.overflow
execute unless score #overflow_count reg_pressure matches 0 run return fail
execute unless score #roots reg_pressure = #seed_length reg_pressure run return fail
execute unless score #activations reg_pressure = #activation_target reg_pressure run return fail
scoreboard players operation #expected_folds reg_pressure = #requested_width reg_pressure
scoreboard players operation #expected_folds reg_pressure *= #activation_target reg_pressure
execute unless score #folds reg_pressure = #expected_folds reg_pressure run return fail

execute store result storage register_pressure:state work.result.width int 1 run scoreboard players get #requested_width reg_pressure
execute store result storage register_pressure:state work.result.checksum int 1 run scoreboard players get #checksum reg_pressure
execute unless data storage register_pressure:state work.result.width run return fail
execute unless data storage register_pressure:state work.result.checksum run return fail
data modify storage worldless_lab:register_pressure/output width set from storage register_pressure:state work.result.width
data modify storage worldless_lab:register_pressure/output checksum set from storage register_pressure:state work.result.checksum
return run execute if data storage worldless_lab:register_pressure/output width if data storage worldless_lab:register_pressure/output checksum
