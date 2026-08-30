data remove storage worldless_lab:int_sort/output values
data modify storage worldless_lab:int_sort/output values set from storage int_sort:state work.values
return run execute if data storage worldless_lab:int_sort/output values
