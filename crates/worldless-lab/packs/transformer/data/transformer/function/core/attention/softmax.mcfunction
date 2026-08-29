execute store result storage transformer:runtime state.macro.score_index int 1 run scoreboard players get #score_index transformer
scoreboard players set #score transformer 0
function transformer:core/attention/load_score.macro with storage transformer:runtime state.macro
scoreboard players operation #delta_score transformer = #score transformer
scoreboard players operation #delta_score transformer -= #score_max transformer
execute if score #delta_score transformer < #softmax_min transformer run scoreboard players operation #delta_score transformer = #softmax_min transformer
scoreboard players operation #lut_index transformer = #delta_score transformer
scoreboard players operation #lut_index transformer -= #softmax_min transformer
execute store result storage transformer:runtime state.macro.lut_index int 1 run scoreboard players get #lut_index transformer
scoreboard players set #weight transformer 0
function transformer:core/attention/load_softmax.macro with storage transformer:runtime state.macro
function transformer:core/attention/store_weight.macro with storage transformer:runtime state.macro
scoreboard players operation #weight_sum transformer += #weight transformer
scoreboard players add #score_index transformer 1
execute if score #score_index transformer <= #cache_last transformer run function transformer:core/attention/softmax
