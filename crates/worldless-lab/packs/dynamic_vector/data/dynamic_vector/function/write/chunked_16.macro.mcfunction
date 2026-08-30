$execute unless data storage dynamic_vector:state work.pages[$(page)][$(offset)] run return fail
$data modify storage dynamic_vector:state work.pages[$(page)][$(offset)] set from storage dynamic_vector:state work.value
return 1
