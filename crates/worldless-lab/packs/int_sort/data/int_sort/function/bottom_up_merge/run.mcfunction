scoreboard players set #width int_sort 1
scoreboard players set #two int_sort 2
execute if score #width int_sort < #length int_sort run function int_sort:bottom_up_merge/pass
