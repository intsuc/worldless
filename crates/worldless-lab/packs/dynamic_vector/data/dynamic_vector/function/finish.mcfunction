data modify storage dynamic_vector:state work.result set value {length:0,checksum:0}
execute store result storage dynamic_vector:state work.result.length int 1 run scoreboard players get #length dynamic_vector
execute store result storage dynamic_vector:state work.result.checksum int 1 run scoreboard players get #checksum dynamic_vector
data remove storage worldless_lab:dynamic_vector/output length
data remove storage worldless_lab:dynamic_vector/output checksum
data modify storage worldless_lab:dynamic_vector/output length set from storage dynamic_vector:state work.result.length
data modify storage worldless_lab:dynamic_vector/output checksum set from storage dynamic_vector:state work.result.checksum
return run execute if data storage worldless_lab:dynamic_vector/output length if data storage worldless_lab:dynamic_vector/output checksum
