data remove storage transformer:response ok
data remove storage transformer:response error
data remove storage transformer:response generated
data remove storage transformer:response final_hidden
data modify storage transformer:response ok set value 0b
function transformer:model/validate
execute unless score #valid transformer matches 1 run data modify storage transformer:response error set value 1
execute unless score #valid transformer matches 1 run return fail

data modify storage transformer:validation tokenizer_left set from storage transformer:constants tokenizer_id
data modify storage transformer:validation tokenizer_right set from storage transformer:model abi.tokenizer_id
function transformer:infer/validate_tokenizer_id
execute unless score #tokenizer_match transformer matches 1 run data modify storage transformer:response error set value 2
execute unless score #tokenizer_match transformer matches 1 run return fail
function transformer:infer/validate_text_request
execute unless score #request_valid transformer matches 1 run data modify storage transformer:response error set value 3
execute unless score #request_valid transformer matches 1 run return fail

data remove storage transformer:runtime state
data modify storage transformer:runtime state set value {tokens:[I;],generated:[I;],cache:[],macro:{}}
data modify storage transformer:runtime state.tokens append from storage transformer:model abi.bos_id
data modify storage transformer:runtime state.remaining set string storage transformer:request prefix
function transformer:tokenize/run
execute unless score #token_error transformer matches 0 run data modify storage transformer:response error set value 4
execute unless score #token_error transformer matches 0 run return fail
execute store result score #token_count transformer run data get storage transformer:runtime state.tokens
function transformer:infer/check_context
execute unless score #request_valid transformer matches 1 run data modify storage transformer:response error set value 6
execute unless score #request_valid transformer matches 1 run return fail
function transformer:infer/prepare
data modify storage transformer:response generated set from storage transformer:runtime state.generated
data modify storage transformer:response final_hidden set from storage transformer:runtime state.norm
data modify storage transformer:response ok set value 1b
return 1
