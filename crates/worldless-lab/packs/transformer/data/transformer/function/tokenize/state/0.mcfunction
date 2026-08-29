execute if data storage transformer:runtime {state:{scan:""}} run return 1
data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 1
execute if data storage transformer:runtime {state:{ch:" "}} run return run function transformer:tokenize/state/1
data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 1
execute if data storage transformer:runtime {state:{ch:"a"}} run return run function transformer:tokenize/state/2
data modify storage transformer:runtime state.ch set string storage transformer:runtime state.scan 0 1
execute if data storage transformer:runtime {state:{ch:"b"}} run return run function transformer:tokenize/state/257
return 0
