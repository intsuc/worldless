scoreboard players set #request_valid transformer 1
scoreboard players operation #required_context transformer = #token_count transformer
scoreboard players operation #required_context transformer += #max_new transformer
scoreboard players remove #required_context transformer 1
execute if score #required_context transformer > #context transformer run scoreboard players set #request_valid transformer 0
