function call_frames:prepare
execute unless score #valid call_frames matches 1 run return fail
function call_frames:driver/static_scores
return run function call_frames:finish
