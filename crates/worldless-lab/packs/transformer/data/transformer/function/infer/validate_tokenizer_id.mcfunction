scoreboard players set #tokenizer_match transformer 1
scoreboard players set #tokenizer_index transformer 0
function transformer:infer/validate_tokenizer_word
return run scoreboard players get #tokenizer_match transformer
