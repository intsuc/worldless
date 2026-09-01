$data modify storage transformer:runtime state.scores[$(key)] set compute default integer {type:score,target:{type:fixed,name:"#score"},score:"transformer"}
