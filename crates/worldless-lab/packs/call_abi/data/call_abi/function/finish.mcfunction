execute store result storage call_abi:state work.result.checksum int 1 run scoreboard players get #checksum call_abi
data remove storage worldless_lab:call_abi/output checksum
data modify storage worldless_lab:call_abi/output checksum set from storage call_abi:state work.result.checksum
return run execute if data storage worldless_lab:call_abi/output checksum
