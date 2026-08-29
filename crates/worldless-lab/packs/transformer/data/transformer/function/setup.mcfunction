scoreboard objectives add transformer dummy
scoreboard players set #zero transformer 0
scoreboard players set #one transformer 1
scoreboard players set #two transformer 2
scoreboard players set #-one transformer -1
scoreboard players set #vocab transformer 512
scoreboard players set #bos transformer 510
scoreboard players set #eos transformer 511
scoreboard players set #layers transformer 4
scoreboard players set #d_model transformer 96
scoreboard players set #q_heads transformer 6
scoreboard players set #head_dim transformer 16
scoreboard players set #d_ff transformer 192
scoreboard players set #context transformer 256
scoreboard players set #window transformer 64
scoreboard players set #cache_last transformer 63
scoreboard players set #softmax_min transformer -255
function transformer:constants/load
