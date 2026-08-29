data modify storage transformer:runtime state.scan set string storage transformer:runtime state.scan 1
data modify storage transformer:runtime state.best_id set value 253
data modify storage transformer:runtime state.best_remaining set from storage transformer:runtime state.scan
execute if data storage transformer:runtime {state:{scan:""}} run return 1
data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 1
execute if data storage transformer:runtime {state:{ch:"a"}} run return run function transformer:tokenize/state/507
data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 1
execute if data storage transformer:runtime {state:{ch:"b"}} run return run function transformer:tokenize/state/508
return 1
