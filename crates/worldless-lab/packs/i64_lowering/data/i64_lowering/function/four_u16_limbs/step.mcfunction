execute store success score #less i64_lowering if score #x3 i64_lowering < #y3 i64_lowering
scoreboard players set #compare i64_lowering 0
execute store success score #compare i64_lowering if score #x3 i64_lowering = #y3 i64_lowering if score #x2 i64_lowering < #y2 i64_lowering
scoreboard players operation #less i64_lowering > #compare i64_lowering
scoreboard players set #compare i64_lowering 0
execute store success score #compare i64_lowering if score #x3 i64_lowering = #y3 i64_lowering if score #x2 i64_lowering = #y2 i64_lowering if score #x1 i64_lowering < #y1 i64_lowering
scoreboard players operation #less i64_lowering > #compare i64_lowering
scoreboard players set #compare i64_lowering 0
execute store success score #compare i64_lowering if score #x3 i64_lowering = #y3 i64_lowering if score #x2 i64_lowering = #y2 i64_lowering if score #x1 i64_lowering = #y1 i64_lowering if score #x0 i64_lowering < #y0 i64_lowering
scoreboard players operation #less i64_lowering > #compare i64_lowering
function i64_lowering:accumulate_less

scoreboard players operation #x0 i64_lowering += #y0 i64_lowering
scoreboard players operation #carry i64_lowering = #x0 i64_lowering
scoreboard players operation #carry i64_lowering /= #base i64_lowering
scoreboard players operation #x0 i64_lowering %= #base i64_lowering
scoreboard players operation #x1 i64_lowering += #y1 i64_lowering
scoreboard players operation #x1 i64_lowering += #carry i64_lowering
scoreboard players operation #carry i64_lowering = #x1 i64_lowering
scoreboard players operation #carry i64_lowering /= #base i64_lowering
scoreboard players operation #x1 i64_lowering %= #base i64_lowering
scoreboard players operation #x2 i64_lowering += #y2 i64_lowering
scoreboard players operation #x2 i64_lowering += #carry i64_lowering
scoreboard players operation #carry i64_lowering = #x2 i64_lowering
scoreboard players operation #carry i64_lowering /= #base i64_lowering
scoreboard players operation #x2 i64_lowering %= #base i64_lowering
scoreboard players operation #x3 i64_lowering += #y3 i64_lowering
scoreboard players operation #x3 i64_lowering += #carry i64_lowering
scoreboard players operation #x3 i64_lowering += #sign_bias i64_lowering
scoreboard players operation #x3 i64_lowering %= #base i64_lowering

scoreboard players operation #y0 i64_lowering -= #step0 i64_lowering
scoreboard players operation #borrow i64_lowering = #y0 i64_lowering
scoreboard players operation #borrow i64_lowering /= #base i64_lowering
scoreboard players operation #y0 i64_lowering %= #base i64_lowering
scoreboard players operation #y1 i64_lowering += #borrow i64_lowering
scoreboard players operation #y1 i64_lowering -= #step1 i64_lowering
scoreboard players operation #borrow i64_lowering = #y1 i64_lowering
scoreboard players operation #borrow i64_lowering /= #base i64_lowering
scoreboard players operation #y1 i64_lowering %= #base i64_lowering
scoreboard players operation #y2 i64_lowering += #borrow i64_lowering
scoreboard players operation #y2 i64_lowering -= #step2 i64_lowering
scoreboard players operation #borrow i64_lowering = #y2 i64_lowering
scoreboard players operation #borrow i64_lowering /= #base i64_lowering
scoreboard players operation #y2 i64_lowering %= #base i64_lowering
scoreboard players operation #y3 i64_lowering += #borrow i64_lowering
scoreboard players operation #y3 i64_lowering -= #step3 i64_lowering
scoreboard players operation #y3 i64_lowering %= #base i64_lowering
