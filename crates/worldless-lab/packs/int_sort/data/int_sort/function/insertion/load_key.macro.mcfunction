$data modify storage int_sort:state work.key set from storage int_sort:state work.values[$(i)]
$execute store result score #key int_sort run data get storage int_sort:state work.values[$(i)]
