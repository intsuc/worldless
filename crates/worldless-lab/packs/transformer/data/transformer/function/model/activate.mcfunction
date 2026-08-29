function transformer:model/validate
execute unless score #valid transformer matches 1 run return fail
data modify storage transformer:validation macro.bank set value 0
execute if data storage transformer:runtime {active_bank:0} run data modify storage transformer:validation macro.bank set value 1
function transformer:model/generated/stage_dispatch.macro with storage transformer:validation macro
return run data modify storage transformer:runtime active_bank set from storage transformer:validation macro.bank
