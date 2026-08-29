function transformer:core/generated/rms/sum
scoreboard players operation #mean_square transformer = #sum_square transformer
scoreboard players operation #mean_square transformer /= #d_model transformer
execute store result storage transformer:runtime state.macro.index int 1 run scoreboard players get #mean_square transformer
function transformer:core/rms/load_gain.macro with storage transformer:runtime state.macro
function transformer:core/generated/rms/normalize
