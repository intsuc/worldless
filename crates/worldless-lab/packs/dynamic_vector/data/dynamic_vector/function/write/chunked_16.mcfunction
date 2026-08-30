scoreboard players operation #page dynamic_vector = #index dynamic_vector
scoreboard players operation #page dynamic_vector /= #page_size dynamic_vector
scoreboard players operation #offset dynamic_vector = #index dynamic_vector
scoreboard players operation #offset dynamic_vector %= #page_size dynamic_vector
execute store result storage dynamic_vector:state work.macro.page int 1 run scoreboard players get #page dynamic_vector
execute store result storage dynamic_vector:state work.macro.offset int 1 run scoreboard players get #offset dynamic_vector
return run function dynamic_vector:write/chunked_16.macro with storage dynamic_vector:state work.macro
