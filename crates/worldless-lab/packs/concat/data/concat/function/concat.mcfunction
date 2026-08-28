scoreboard objectives add concat dummy

data remove storage concat: result

execute store result score #expected concat run data get storage concat: first
execute store result score #actual concat run data get storage concat: second
scoreboard players operation #expected concat += #actual concat

function concat:concat/single_quotes.macro with storage concat:
execute store result score #actual concat run data get storage concat: result
execute if score #expected concat = #actual concat if data storage concat: result run return 1

data remove storage concat: result
function concat:concat/double_quotes.macro with storage concat:
execute store result score #actual concat run data get storage concat: result
execute if score #expected concat = #actual concat if data storage concat: result run return 2

data remove storage concat: result
data modify storage concat: tokens set value []
data modify storage concat: decompose set from storage concat: first
function concat:concat/decompose with storage concat:

data modify storage concat: decompose set from storage concat: second
function concat:concat/decompose with storage concat:

function concat:concat/compose
execute if score #failed concat matches 0 run data modify storage concat: result set from storage concat: tokens[0]
data remove storage concat: tokens
return run execute if score #failed concat matches 0 if data storage concat: result
