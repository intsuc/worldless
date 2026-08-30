function dynamic_vector:prepare
execute unless score #valid dynamic_vector matches 1 run return fail
data modify storage dynamic_vector:state work set value {pages:[],value:0,macro:{variant:"chunked_16",layout:"chunked_16",index:0,page:0,offset:0},result:{length:0,checksum:0}}
scoreboard players set #length dynamic_vector 0
scoreboard players set #capacity dynamic_vector 0
execute unless function dynamic_vector:run run return fail
return run function dynamic_vector:finish
