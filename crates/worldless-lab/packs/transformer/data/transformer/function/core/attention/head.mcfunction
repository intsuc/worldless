data modify storage transformer:runtime state.scores set from storage transformer:constants zero64
data modify storage transformer:runtime state.weights set from storage transformer:constants zero64
scoreboard players set #score_max transformer -2147483648
execute store result storage transformer:runtime state.macro.head int 1 run scoreboard players get #head transformer
scoreboard players operation #key transformer = #key_start transformer
function transformer:core/attention/score
scoreboard players operation #score_index transformer = #key_start transformer
scoreboard players set #weight_sum transformer 0
function transformer:core/attention/softmax
scoreboard players operation #divisor transformer = #weight_sum transformer
scoreboard players operation #half transformer = #divisor transformer
scoreboard players operation #half transformer /= #two transformer
function transformer:core/generated/attention/value
scoreboard players add #head transformer 1
execute if score #head transformer < #q_heads transformer run function transformer:core/attention/head
