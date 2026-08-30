function aggregate_layout:prepare
execute unless score #valid aggregate_layout matches 1 run return fail
execute if score #length aggregate_layout matches 1 run function aggregate_layout:length/record_compounds/n1
execute if score #length aggregate_layout matches 16 run function aggregate_layout:length/record_compounds/n16
execute if score #length aggregate_layout matches 64 run function aggregate_layout:length/record_compounds/n64
execute if score #length aggregate_layout matches 128 run function aggregate_layout:length/record_compounds/n128
return run function aggregate_layout:finish
