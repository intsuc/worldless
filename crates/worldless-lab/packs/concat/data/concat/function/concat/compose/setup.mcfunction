# Equal tree depth is required because every merge decodes its inputs once.
scoreboard players set #two concat 2
scoreboard players set #capacity concat 1
scoreboard players set #levels concat 0
function concat:concat/compose/grow
execute if score #width concat < #capacity concat run function concat:concat/compose/pad

# A special leaf needs 2^levels - 1 backslashes to survive every merge.
data modify storage concat: escape set value '\\'
function concat:concat/compose/double_escape
execute if score #failed concat matches 0 run data modify storage concat: escape set string storage concat: escape 1

execute if score #failed concat matches 0 run function concat:concat/compose/first_round

data remove storage concat: next_escape
data remove storage concat: merged
data remove storage concat: next
data remove storage concat: escape
data remove storage concat: left_escape
data remove storage concat: right_escape
data remove storage concat: left_index
data remove storage concat: right_index
data remove storage concat: left
data remove storage concat: right
