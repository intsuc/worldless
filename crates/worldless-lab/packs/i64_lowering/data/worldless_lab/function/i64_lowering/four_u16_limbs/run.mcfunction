function i64_lowering:prepare
execute unless score #valid i64_lowering matches 1 run return fail
data modify storage i64_lowering:state macro.variant set value "four_u16_limbs"
return run function i64_lowering:entry.macro with storage i64_lowering:state macro
