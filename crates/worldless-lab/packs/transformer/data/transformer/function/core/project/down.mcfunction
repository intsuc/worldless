execute store result storage transformer:runtime state.macro.layer int 1 run scoreboard players get #layer transformer
function transformer:core/project/select_down.macro with storage transformer:runtime state.macro
execute store result storage transformer:runtime state.macro.shift int 1 run scoreboard players get #shift transformer
function transformer:core/generated/project/p96x192_relu2
data modify storage transformer:runtime state.ffn_projection set from storage transformer:runtime state.projected
