execute store result storage aggregate_layout:state work.result.checksum int 1 run scoreboard players get #checksum aggregate_layout
data remove storage worldless_lab:aggregate_layout/output checksum
data modify storage worldless_lab:aggregate_layout/output checksum set from storage aggregate_layout:state work.result.checksum
return run execute if data storage worldless_lab:aggregate_layout/output checksum
