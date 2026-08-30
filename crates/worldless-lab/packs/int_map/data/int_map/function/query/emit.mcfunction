execute store result storage int_map:state work.result.found byte 1 run scoreboard players get #found int_map
execute store result storage int_map:state work.result.value int 1 run scoreboard players get #value int_map
data modify storage int_map:state work.output.found append from storage int_map:state work.result.found
data modify storage int_map:state work.output.values append from storage int_map:state work.result.value
