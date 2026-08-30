function register_pressure:prepare
execute unless score #valid reg_pressure matches 1 run return fail
data modify storage register_pressure:state work.macro.variant set value "static_scores"
return run function register_pressure:entry.macro with storage register_pressure:state work.macro
