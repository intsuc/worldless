function call_abi:prepare
execute unless score #valid call_abi matches 1 run return fail
execute if score #length call_abi matches 1 run function call_abi:driver/score_slot/single
execute if score #length call_abi matches 63 run function call_abi:driver/score_slot/many
return run function call_abi:finish
