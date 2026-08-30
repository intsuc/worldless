function numeric_lowering:prepare
execute unless score #valid numeric_lowering matches 1 run return fail
execute if score #a_length numeric_lowering matches 1 run function numeric_lowering:driver/compute_fused/n1
execute if score #a_length numeric_lowering matches 4 run function numeric_lowering:driver/compute_fused/n4
execute if score #a_length numeric_lowering matches 16 run function numeric_lowering:driver/compute_fused/n16
execute if score #a_length numeric_lowering matches 64 run function numeric_lowering:driver/compute_fused/n64
return run function numeric_lowering:finish
