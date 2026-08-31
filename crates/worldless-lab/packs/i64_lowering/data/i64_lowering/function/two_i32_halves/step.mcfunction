execute store success score #less i64_lowering if score #x_high i64_lowering < #y_high i64_lowering
scoreboard players set #compare i64_lowering 0
execute store success score #compare i64_lowering if score #x_high i64_lowering = #y_high i64_lowering if score #x_low i64_lowering < #y_low i64_lowering
scoreboard players operation #less i64_lowering > #compare i64_lowering
function i64_lowering:accumulate_less

scoreboard players operation #old_low i64_lowering = #x_low i64_lowering
scoreboard players operation #x_low i64_lowering += #y_low i64_lowering
scoreboard players operation #x_low i64_lowering += #min i64_lowering
execute store success score #carry i64_lowering if score #x_low i64_lowering < #old_low i64_lowering
scoreboard players operation #x_high i64_lowering += #y_high i64_lowering
scoreboard players operation #x_high i64_lowering += #carry i64_lowering

execute store success score #borrow i64_lowering if score #y_low i64_lowering < #step_low i64_lowering
scoreboard players operation #y_low i64_lowering -= #step_low i64_lowering
scoreboard players operation #y_low i64_lowering += #min i64_lowering
scoreboard players operation #y_high i64_lowering -= #step_high i64_lowering
scoreboard players operation #y_high i64_lowering -= #borrow i64_lowering
