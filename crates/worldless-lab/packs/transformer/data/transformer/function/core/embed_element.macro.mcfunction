$data modify storage transformer:runtime state.hidden append compute default {type:storage,storage:"transformer:model",path:"weights.\"token_embedding.weight\"[$(index)]"} integer
