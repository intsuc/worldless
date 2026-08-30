function aggregate_layout:init/flat_array/n64
execute if score #order aggregate_layout matches 1 run function aggregate_layout:driver/flat_array/record_major/n64
execute if score #order aggregate_layout matches 2 run function aggregate_layout:driver/flat_array/field_major/n64
