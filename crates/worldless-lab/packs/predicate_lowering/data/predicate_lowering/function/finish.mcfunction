execute unless score #evaluations pred_lower = #evaluation_target pred_lower run return fail
execute unless score #result pred_lower matches 0..1 run return fail

data modify storage predicate_lowering:state work.result set value {result:0,checksum:0}
execute store result storage predicate_lowering:state work.result.result int 1 run scoreboard players get #result pred_lower
execute store result storage predicate_lowering:state work.result.checksum int 1 run scoreboard players get #checksum pred_lower
execute unless data storage predicate_lowering:state work.result.result run return fail
execute unless data storage predicate_lowering:state work.result.checksum run return fail
data modify storage worldless_lab:predicate_lowering/output result set from storage predicate_lowering:state work.result.result
data modify storage worldless_lab:predicate_lowering/output checksum set from storage predicate_lowering:state work.result.checksum
return run execute if data storage worldless_lab:predicate_lowering/output result if data storage worldless_lab:predicate_lowering/output checksum
