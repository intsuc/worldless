function tagged_union_layout:prepare
execute unless score #prepared tag_union matches 1 run return fail
function tagged_union_layout:driver/narrow_array
return run function tagged_union_layout:finish
