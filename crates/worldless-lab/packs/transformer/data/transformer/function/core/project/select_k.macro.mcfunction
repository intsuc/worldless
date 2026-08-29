$data modify storage transformer:runtime state.matrix set from storage transformer:model weights."blocks.$(layer).attention.k_proj.weight"
$execute store result score #shift transformer run data get storage transformer:model shifts."blocks.$(layer).attention.k_proj.weight"[0]
