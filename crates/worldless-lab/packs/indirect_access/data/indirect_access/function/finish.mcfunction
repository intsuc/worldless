execute store result storage indirect_access:state work.result.checksum int 1 run scoreboard players get #checksum indirect_access
data remove storage worldless_lab:indirect_access/output checksum
data modify storage worldless_lab:indirect_access/output checksum set from storage indirect_access:state work.result.checksum
return run execute if data storage worldless_lab:indirect_access/output checksum
