execute unless score #remaining loop_lowering matches 0 run return fail
execute unless score #executed loop_lowering = #requested loop_lowering run return fail

execute store result storage loop_lowering:state work.result.iterations int 1 run scoreboard players get #executed loop_lowering
execute store result storage loop_lowering:state work.result.value int 1 run scoreboard players get #value loop_lowering
execute store result storage loop_lowering:state work.result.checksum int 1 run scoreboard players get #checksum loop_lowering
execute unless data storage loop_lowering:state work.result.iterations run return fail
execute unless data storage loop_lowering:state work.result.value run return fail
execute unless data storage loop_lowering:state work.result.checksum run return fail

data modify storage worldless_lab:loop_lowering/output iterations set from storage loop_lowering:state work.result.iterations
data modify storage worldless_lab:loop_lowering/output value set from storage loop_lowering:state work.result.value
data modify storage worldless_lab:loop_lowering/output checksum set from storage loop_lowering:state work.result.checksum
return run execute if data storage worldless_lab:loop_lowering/output iterations if data storage worldless_lab:loop_lowering/output value if data storage worldless_lab:loop_lowering/output checksum
