function int_sort:prepare
execute unless score #valid int_sort matches 1 run return fail
function int_sort:bottom_up_merge/run
return run function int_sort:finish
