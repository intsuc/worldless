data remove storage worldless_lab:concat/output result
data modify storage concat: first set from storage worldless_lab:concat/input first
data modify storage concat: second set from storage worldless_lab:concat/input second
function concat:concat
data modify storage worldless_lab:concat/output result set from storage concat: result
return run execute if data storage worldless_lab:concat/output result
