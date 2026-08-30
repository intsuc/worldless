function aggregate_layout:init/column_arrays/n16
execute if score #order aggregate_layout matches 1 run function aggregate_layout:driver/column_arrays/record_major/n16
execute if score #order aggregate_layout matches 2 run function aggregate_layout:driver/column_arrays/field_major/n16
