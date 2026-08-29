scoreboard players operation #weight_index transformer = #token transformer
scoreboard players operation #weight_index transformer *= #d_model transformer
scoreboard players operation #weight_index transformer += #dim transformer
execute store result storage transformer:runtime state.macro.index int 1 run scoreboard players get #weight_index transformer
function transformer:core/embed_element.macro with storage transformer:runtime state.macro
scoreboard players add #dim transformer 1
execute if score #dim transformer < #d_model transformer run function transformer:core/embed_element
