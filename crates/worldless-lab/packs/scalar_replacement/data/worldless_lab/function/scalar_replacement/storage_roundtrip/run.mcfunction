function scalar_replacement:prepare
execute unless score #valid scalar_replace matches 1 run return fail
data modify storage scalar_replacement:state macro.variant set value "storage_roundtrip"
return run function scalar_replacement:entry.macro with storage scalar_replacement:state macro
