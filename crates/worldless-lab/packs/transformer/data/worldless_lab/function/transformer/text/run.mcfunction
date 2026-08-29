function transformer:setup
function transformer:fixture/load_model
execute store success score #activate_result transformer run function transformer:model/activate
execute unless score #activate_result transformer matches 1 run return fail
data modify storage transformer:model abi.schema set value 0
execute store success score #activate_result transformer run function transformer:model/activate
execute unless score #activate_result transformer matches 0 run return fail
data remove storage transformer:request prefix
data remove storage transformer:request prefix_tokens
data remove storage transformer:request tokenizer_id
data modify storage transformer:request prefix set from storage worldless_lab:transformer/input prefix
data modify storage transformer:request max_new_tokens set from storage worldless_lab:transformer/input max_new_tokens
execute store success score #infer_result transformer run function transformer:infer/text
data remove storage worldless_lab:transformer/output ok
data remove storage worldless_lab:transformer/output error
data remove storage worldless_lab:transformer/output generated
data remove storage worldless_lab:transformer/output final_hidden
data modify storage worldless_lab:transformer/output ok set from storage transformer:response ok
data modify storage worldless_lab:transformer/output error set from storage transformer:response error
data modify storage worldless_lab:transformer/output generated set from storage transformer:response generated
data modify storage worldless_lab:transformer/output final_hidden set from storage transformer:response final_hidden
execute if data storage transformer:response {ok:1b} unless score #infer_result transformer matches 1 run return fail
execute if data storage transformer:response {ok:0b} unless score #infer_result transformer matches 0 run return fail
execute unless data storage transformer:response {ok:1b} unless data storage transformer:response {ok:0b} run return fail
return 1
