data remove storage transformer:response ok
data remove storage transformer:response error
data remove storage transformer:response generated
data remove storage transformer:response final_hidden
data modify storage transformer:response ok set value 0b
execute unless data storage transformer:runtime {active_bank:0} unless data storage transformer:runtime {active_bank:1} run data modify storage transformer:response error set value 1
execute unless data storage transformer:runtime {active_bank:0} unless data storage transformer:runtime {active_bank:1} run return fail
data modify storage transformer:validation macro.bank set from storage transformer:runtime active_bank
function transformer:infer/validate_tokens_request
execute unless score #request_valid transformer matches 1 run data modify storage transformer:response error set value 3
execute unless score #request_valid transformer matches 1 run return fail

data modify storage transformer:validation tokenizer_left set from storage transformer:request tokenizer_id
function transformer:infer/load_active_tokenizer.macro with storage transformer:validation macro
function transformer:infer/validate_tokenizer_id
execute unless score #tokenizer_match transformer matches 1 run data modify storage transformer:response error set value 3
execute unless score #tokenizer_match transformer matches 1 run return fail
scoreboard players set #token_index transformer 0
scoreboard players set #token_error transformer 0
execute if score #prefix_len transformer matches 1.. run function transformer:infer/validate_tokens
execute unless score #token_error transformer matches 0 run data modify storage transformer:response error set value 5
execute unless score #token_error transformer matches 0 run return fail
scoreboard players operation #token_count transformer = #prefix_len transformer
scoreboard players add #token_count transformer 1
function transformer:infer/check_context
execute unless score #request_valid transformer matches 1 run data modify storage transformer:response error set value 6
execute unless score #request_valid transformer matches 1 run return fail

data remove storage transformer:runtime state
data modify storage transformer:runtime state set value {tokens:[I;],generated:[I;],cache:[],macro:{}}
data modify storage transformer:runtime state.macro.bank set from storage transformer:runtime active_bank
function transformer:infer/load_active_bos.macro with storage transformer:runtime state.macro
execute if score #prefix_len transformer matches 1.. run data modify storage transformer:runtime state.tokens append from storage transformer:request prefix_tokens[]
function transformer:infer/prepare
data modify storage transformer:response generated set from storage transformer:runtime state.generated
data modify storage transformer:response final_hidden set from storage transformer:runtime state.norm
data modify storage transformer:response ok set value 1b
return 1
