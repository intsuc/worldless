$execute store success score #found int_map if data storage int_map:state work.table."$(key)"
$execute if score #found int_map matches 1 store result score #value int_map run data get storage int_map:state work.table."$(key)"
