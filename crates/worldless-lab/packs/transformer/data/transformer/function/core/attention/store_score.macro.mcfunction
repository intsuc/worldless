$data modify storage transformer:runtime state.scores[$(key)] set compute default {type:score,target:{type:fixed,name:"#score"},score:"transformer"} integer
