function int_map:prepare
execute unless score #valid int_map matches 1 run return fail
function int_map:scoreboard/run
return run function int_map:finish
