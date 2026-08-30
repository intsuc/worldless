$data remove storage int_sort:state work.values[$(i)]
$data modify storage int_sort:state work.values insert $(destination) from storage int_sort:state work.key
