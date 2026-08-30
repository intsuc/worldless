function result_abi:prepare
execute unless score #valid result_abi matches 1 run return fail
data modify storage result_abi:state work.macro.variant set value "caller_out"
return run function result_abi:entry.macro with storage result_abi:state work.macro
