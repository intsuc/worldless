function indirect_access:prepare
execute unless score #valid indirect_access matches 1 run return fail
data modify storage indirect_access:state work.macro.variant set value "dynamic_path/access.macro"
function indirect_access:query/run
return run function indirect_access:finish
