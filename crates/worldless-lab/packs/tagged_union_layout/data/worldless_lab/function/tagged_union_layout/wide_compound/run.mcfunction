function tagged_union_layout:prepare
execute unless score #prepared tag_union matches 1 run return fail
function tagged_union_layout:driver/wide_compound
return run function tagged_union_layout:finish
