function aggregate_layout:prepare
execute unless score #valid aggregate_layout matches 1 run return fail
execute if score #length aggregate_layout matches 1 run function aggregate_layout:length/flat_array/n1
execute if score #length aggregate_layout matches 16 run function aggregate_layout:length/flat_array/n16
execute if score #length aggregate_layout matches 64 run function aggregate_layout:length/flat_array/n64
execute if score #length aggregate_layout matches 128 run function aggregate_layout:length/flat_array/n128
return run function aggregate_layout:finish
