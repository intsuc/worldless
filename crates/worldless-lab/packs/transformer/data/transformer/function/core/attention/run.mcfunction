data modify storage transformer:runtime state.attention set value [I;]
scoreboard players operation #key_start transformer = #cache_last transformer
scoreboard players operation #key_start transformer -= #position transformer
execute if score #key_start transformer matches ..-1 run scoreboard players set #key_start transformer 0
scoreboard players set #head transformer 0
function transformer:core/attention/head
