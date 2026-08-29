execute store result storage transformer:runtime state.macro.key int 1 run scoreboard players get #key transformer
function transformer:core/generated/attention/qk_dispatch.macro with storage transformer:runtime state.macro
function transformer:core/attention/store_score.macro with storage transformer:runtime state.macro
execute if score #score transformer > #score_max transformer run scoreboard players operation #score_max transformer = #score transformer
scoreboard players add #key transformer 1
execute if score #key transformer <= #cache_last transformer run function transformer:core/attention/score
