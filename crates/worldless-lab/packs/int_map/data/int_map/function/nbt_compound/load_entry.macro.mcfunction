$data modify storage int_map:state work.entry.key set from storage int_map:state work.keys[$(entry_index)]
$data modify storage int_map:state work.entry.value set from storage int_map:state work.values[$(entry_index)]
