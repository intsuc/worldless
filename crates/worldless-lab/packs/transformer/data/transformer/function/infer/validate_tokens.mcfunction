execute store result storage transformer:validation macro.index int 1 run scoreboard players get #token_index transformer
scoreboard players set #token transformer -1
function transformer:infer/load_request_token.macro with storage transformer:validation macro
execute unless score #token transformer matches 0.. run scoreboard players set #token_error transformer 1
execute if score #token transformer >= #bos transformer run scoreboard players set #token_error transformer 1
scoreboard players add #token_index transformer 1
execute if score #token_error transformer matches 0 if score #token_index transformer < #prefix_len transformer run function transformer:infer/validate_tokens
