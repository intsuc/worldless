data modify storage int_sort:state work.next set value [I;]
scoreboard players set #left int_sort 0
function int_sort:bottom_up_merge/pair
data modify storage int_sort:state work.values set from storage int_sort:state work.next
scoreboard players operation #width int_sort *= #two int_sort
execute if score #width int_sort < #length int_sort run return run function int_sort:bottom_up_merge/pass
