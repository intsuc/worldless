function predicate_lowering:prepare
execute unless score #prepared pred_lower matches 1 run return fail
function predicate_lowering:driver/score_product
return run function predicate_lowering:finish
