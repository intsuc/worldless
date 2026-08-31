data modify storage i64_lowering:state work.result set value {x:[I;0,0],y:[I;0,0],less_count:0}
execute store result storage i64_lowering:state work.result.x[0] int 1 run scoreboard players get #result_x_high i64_lowering
execute store result storage i64_lowering:state work.result.x[1] int 1 run scoreboard players get #result_x_low i64_lowering
execute store result storage i64_lowering:state work.result.y[0] int 1 run scoreboard players get #result_y_high i64_lowering
execute store result storage i64_lowering:state work.result.y[1] int 1 run scoreboard players get #result_y_low i64_lowering
execute store result storage i64_lowering:state work.result.less_count int 1 run scoreboard players get #less_count i64_lowering
data remove storage worldless_lab:i64_lowering/output x
data remove storage worldless_lab:i64_lowering/output y
data remove storage worldless_lab:i64_lowering/output less_count
data modify storage worldless_lab:i64_lowering/output x set from storage i64_lowering:state work.result.x
data modify storage worldless_lab:i64_lowering/output y set from storage i64_lowering:state work.result.y
data modify storage worldless_lab:i64_lowering/output less_count set from storage i64_lowering:state work.result.less_count
return run execute if data storage worldless_lab:i64_lowering/output x if data storage worldless_lab:i64_lowering/output y if data storage worldless_lab:i64_lowering/output less_count
