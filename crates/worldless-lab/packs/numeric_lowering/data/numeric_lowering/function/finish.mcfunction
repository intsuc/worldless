execute store result storage numeric_lowering:state work.result.checksum int 1 run scoreboard players get #checksum numeric_lowering
data remove storage worldless_lab:numeric_lowering/output checksum
data modify storage worldless_lab:numeric_lowering/output checksum set from storage numeric_lowering:state work.result.checksum
return run execute if data storage worldless_lab:numeric_lowering/output checksum
