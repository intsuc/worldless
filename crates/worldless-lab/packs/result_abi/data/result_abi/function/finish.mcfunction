scoreboard players set #frame_count result_abi -1
execute store result score #frame_count result_abi run data get storage result_abi:state work.frames
execute unless score #frame_count result_abi matches 0 run return fail
execute unless score #calls result_abi = #call_target result_abi run return fail
execute unless score #values result_abi = #expected_values result_abi run return fail
execute unless score #folds result_abi = #expected_values result_abi run return fail

execute store result storage result_abi:state work.result.width int 1 run scoreboard players get #requested_width result_abi
execute store result storage result_abi:state work.result.checksum int 1 run scoreboard players get #checksum result_abi
execute unless data storage result_abi:state work.result.width run return fail
execute unless data storage result_abi:state work.result.checksum run return fail
data modify storage worldless_lab:result_abi/output width set from storage result_abi:state work.result.width
data modify storage worldless_lab:result_abi/output checksum set from storage result_abi:state work.result.checksum
return run execute if data storage worldless_lab:result_abi/output width if data storage worldless_lab:result_abi/output checksum
