execute if score #length dynamic_vector = #capacity dynamic_vector store success score #access_ok dynamic_vector run data modify storage dynamic_vector:state work.pages append value [I;0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]
execute if score #length dynamic_vector = #capacity dynamic_vector unless score #access_ok dynamic_vector matches 1 run return fail
execute if score #length dynamic_vector = #capacity dynamic_vector run scoreboard players add #capacity dynamic_vector 16
scoreboard players operation #page dynamic_vector = #length dynamic_vector
scoreboard players operation #page dynamic_vector /= #page_size dynamic_vector
scoreboard players operation #offset dynamic_vector = #length dynamic_vector
scoreboard players operation #offset dynamic_vector %= #page_size dynamic_vector
execute store result storage dynamic_vector:state work.macro.page int 1 run scoreboard players get #page dynamic_vector
execute store result storage dynamic_vector:state work.macro.offset int 1 run scoreboard players get #offset dynamic_vector
return run function dynamic_vector:write/chunked_16.macro with storage dynamic_vector:state work.macro
