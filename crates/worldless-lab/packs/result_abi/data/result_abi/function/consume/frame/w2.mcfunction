execute store result score #value result_abi run data get storage result_abi:state work.frames[-1].v0
scoreboard players operation #checksum result_abi *= #checksum_multiplier result_abi
scoreboard players operation #checksum result_abi += #value result_abi
scoreboard players add #folds result_abi 1
execute store result score #value result_abi run data get storage result_abi:state work.frames[-1].v1
scoreboard players operation #checksum result_abi *= #checksum_multiplier result_abi
scoreboard players operation #checksum result_abi += #value result_abi
scoreboard players add #folds result_abi 1
data remove storage result_abi:state work.frames[-1]
