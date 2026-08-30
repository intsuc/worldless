function aggregate_layout:init/record_compounds/n1
execute if score #order aggregate_layout matches 1 run function aggregate_layout:driver/record_compounds/record_major/n1
execute if score #order aggregate_layout matches 2 run function aggregate_layout:driver/record_compounds/field_major/n1
