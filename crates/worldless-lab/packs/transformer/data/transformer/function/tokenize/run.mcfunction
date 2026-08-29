scoreboard players set #token_error transformer 0
execute unless data storage transformer:runtime {state:{remaining:""}} run function transformer:tokenize/loop
return run execute if score #token_error transformer matches 0
