data remove storage worldless_lab:int_map/output found
data remove storage worldless_lab:int_map/output values
data modify storage worldless_lab:int_map/output found set from storage int_map:state work.output.found
data modify storage worldless_lab:int_map/output values set from storage int_map:state work.output.values
execute unless data storage worldless_lab:int_map/output found run return 0
return run execute if data storage worldless_lab:int_map/output values
