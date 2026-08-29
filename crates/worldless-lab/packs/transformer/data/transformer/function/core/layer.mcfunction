function transformer:core/rms/run
function transformer:core/project/q
function transformer:core/project/k
function transformer:core/project/v
execute store result storage transformer:runtime state.macro.layer int 1 run scoreboard players get #layer transformer
function transformer:core/load_layer_cache.macro with storage transformer:runtime state.macro
data remove storage transformer:runtime state.layer_cache.k[0]
data remove storage transformer:runtime state.layer_cache.v[0]
data modify storage transformer:runtime state.layer_cache.k append from storage transformer:runtime state.k
data modify storage transformer:runtime state.layer_cache.v append from storage transformer:runtime state.v
function transformer:core/attention/run
function transformer:core/save_layer_cache.macro with storage transformer:runtime state.macro
function transformer:core/project/o
data modify storage transformer:runtime state.delta set from storage transformer:runtime state.attention_projection
function transformer:core/residual

function transformer:core/rms/run
function transformer:core/project/up
function transformer:core/project/down
data modify storage transformer:runtime state.delta set from storage transformer:runtime state.ffn_projection
function transformer:core/residual

scoreboard players add #layer transformer 1
execute if score #layer transformer < #layers transformer run function transformer:core/layer
