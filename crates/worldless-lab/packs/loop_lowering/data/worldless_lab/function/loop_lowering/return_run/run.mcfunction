function loop_lowering:prepare
execute unless score #valid loop_lowering matches 1 run return fail
data modify storage loop_lowering:state work.macro.variant set value "return_run"
return run function loop_lowering:entry.macro with storage loop_lowering:state work.macro
