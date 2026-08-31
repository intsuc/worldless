scoreboard players set #actual_width scalar_replace -1
execute store result score #actual_width scalar_replace run data get storage scalar_replacement:state work.values
execute unless score #actual_width scalar_replace = #width scalar_replace run return fail
execute unless data storage scalar_replacement:state work.result.checksum run return fail

data remove storage worldless_lab:scalar_replacement/output checksum
data modify storage worldless_lab:scalar_replacement/output checksum set from storage scalar_replacement:state work.result.checksum
return run execute if data storage worldless_lab:scalar_replacement/output checksum
