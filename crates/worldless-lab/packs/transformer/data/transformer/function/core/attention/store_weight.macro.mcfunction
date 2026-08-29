$data modify storage transformer:runtime state.weights[$(score_index)] set compute default {type:score,target:{type:fixed,name:"#weight"},score:"transformer"} integer
