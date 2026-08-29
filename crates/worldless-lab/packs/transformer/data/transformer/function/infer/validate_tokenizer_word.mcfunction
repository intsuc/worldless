execute store result storage transformer:validation macro.index int 1 run scoreboard players get #tokenizer_index transformer
function transformer:infer/load_tokenizer_words.macro with storage transformer:validation macro
execute unless score #request_word transformer = #model_word transformer run scoreboard players set #tokenizer_match transformer 0
scoreboard players add #tokenizer_index transformer 1
execute if score #tokenizer_match transformer matches 1 if score #tokenizer_index transformer matches ..7 run function transformer:infer/validate_tokenizer_word
