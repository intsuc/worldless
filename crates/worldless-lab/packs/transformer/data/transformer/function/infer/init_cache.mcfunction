data modify storage transformer:runtime state.layer_cache set value {k:[],v:[]}
data modify storage transformer:runtime state.layer_cache.k set from storage transformer:constants zero_kv
data modify storage transformer:runtime state.layer_cache.v set from storage transformer:constants zero_kv
data modify storage transformer:runtime state.cache append from storage transformer:runtime state.layer_cache
scoreboard players add #layer transformer 1
execute if score #layer transformer < #layers transformer run function transformer:infer/init_cache
