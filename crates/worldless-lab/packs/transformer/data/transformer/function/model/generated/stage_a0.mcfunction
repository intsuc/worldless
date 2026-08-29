data modify storage transformer:a0 e set from storage transformer:model weights."token_embedding.weight"
data modify storage transformer:a0 t set from storage transformer:model abi.tokenizer_id
data modify storage transformer:a0 b set from storage transformer:model abi.bos_id
data modify storage transformer:a0 w0q set from storage transformer:model weights."blocks.0.attention.q_proj.weight"
data modify storage transformer:a0 a0q set value {"s":"transformer:a0","w":"w0q"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.attention.q_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0q.rq set from storage transformer:validation rq
data modify storage transformer:a0 w0k set from storage transformer:model weights."blocks.0.attention.k_proj.weight"
data modify storage transformer:a0 a0k set value {"s":"transformer:a0","w":"w0k"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.attention.k_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0k.rq set from storage transformer:validation rq
data modify storage transformer:a0 w0v set from storage transformer:model weights."blocks.0.attention.v_proj.weight"
data modify storage transformer:a0 a0v set value {"s":"transformer:a0","w":"w0v"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.attention.v_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0v.rq set from storage transformer:validation rq
data modify storage transformer:a0 w0o set from storage transformer:model weights."blocks.0.attention.out_proj.weight"
data modify storage transformer:a0 a0o set value {"s":"transformer:a0","w":"w0o"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.attention.out_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0o.rq set from storage transformer:validation rq
data modify storage transformer:a0 w0u set from storage transformer:model weights."blocks.0.ffn.up_proj.weight"
data modify storage transformer:a0 a0u set value {"s":"transformer:a0","w":"w0u"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.ffn.up_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0u.rq set from storage transformer:validation rq
data modify storage transformer:a0 w0d set from storage transformer:model weights."blocks.0.ffn.down_proj.weight"
data modify storage transformer:a0 a0d set value {"s":"transformer:a0","w":"w0d"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.0.ffn.down_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a0d.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1q set from storage transformer:model weights."blocks.1.attention.q_proj.weight"
data modify storage transformer:a0 a1q set value {"s":"transformer:a0","w":"w1q"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.attention.q_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1q.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1k set from storage transformer:model weights."blocks.1.attention.k_proj.weight"
data modify storage transformer:a0 a1k set value {"s":"transformer:a0","w":"w1k"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.attention.k_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1k.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1v set from storage transformer:model weights."blocks.1.attention.v_proj.weight"
data modify storage transformer:a0 a1v set value {"s":"transformer:a0","w":"w1v"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.attention.v_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1v.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1o set from storage transformer:model weights."blocks.1.attention.out_proj.weight"
data modify storage transformer:a0 a1o set value {"s":"transformer:a0","w":"w1o"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.attention.out_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1o.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1u set from storage transformer:model weights."blocks.1.ffn.up_proj.weight"
data modify storage transformer:a0 a1u set value {"s":"transformer:a0","w":"w1u"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.ffn.up_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1u.rq set from storage transformer:validation rq
data modify storage transformer:a0 w1d set from storage transformer:model weights."blocks.1.ffn.down_proj.weight"
data modify storage transformer:a0 a1d set value {"s":"transformer:a0","w":"w1d"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.1.ffn.down_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a1d.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2q set from storage transformer:model weights."blocks.2.attention.q_proj.weight"
data modify storage transformer:a0 a2q set value {"s":"transformer:a0","w":"w2q"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.attention.q_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2q.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2k set from storage transformer:model weights."blocks.2.attention.k_proj.weight"
data modify storage transformer:a0 a2k set value {"s":"transformer:a0","w":"w2k"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.attention.k_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2k.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2v set from storage transformer:model weights."blocks.2.attention.v_proj.weight"
data modify storage transformer:a0 a2v set value {"s":"transformer:a0","w":"w2v"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.attention.v_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2v.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2o set from storage transformer:model weights."blocks.2.attention.out_proj.weight"
data modify storage transformer:a0 a2o set value {"s":"transformer:a0","w":"w2o"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.attention.out_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2o.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2u set from storage transformer:model weights."blocks.2.ffn.up_proj.weight"
data modify storage transformer:a0 a2u set value {"s":"transformer:a0","w":"w2u"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.ffn.up_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2u.rq set from storage transformer:validation rq
data modify storage transformer:a0 w2d set from storage transformer:model weights."blocks.2.ffn.down_proj.weight"
data modify storage transformer:a0 a2d set value {"s":"transformer:a0","w":"w2d"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.2.ffn.down_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a2d.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3q set from storage transformer:model weights."blocks.3.attention.q_proj.weight"
data modify storage transformer:a0 a3q set value {"s":"transformer:a0","w":"w3q"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.attention.q_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3q.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3k set from storage transformer:model weights."blocks.3.attention.k_proj.weight"
data modify storage transformer:a0 a3k set value {"s":"transformer:a0","w":"w3k"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.attention.k_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3k.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3v set from storage transformer:model weights."blocks.3.attention.v_proj.weight"
data modify storage transformer:a0 a3v set value {"s":"transformer:a0","w":"w3v"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.attention.v_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3v.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3o set from storage transformer:model weights."blocks.3.attention.out_proj.weight"
data modify storage transformer:a0 a3o set value {"s":"transformer:a0","w":"w3o"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.attention.out_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3o.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3u set from storage transformer:model weights."blocks.3.ffn.up_proj.weight"
data modify storage transformer:a0 a3u set value {"s":"transformer:a0","w":"w3u"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.ffn.up_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3u.rq set from storage transformer:validation rq
data modify storage transformer:a0 w3d set from storage transformer:model weights."blocks.3.ffn.down_proj.weight"
data modify storage transformer:a0 a3d set value {"s":"transformer:a0","w":"w3d"}
data modify storage transformer:validation macro.shift set from storage transformer:model shifts."blocks.3.ffn.down_proj.weight"[0]
function transformer:model/generated/stage_shift_dispatch.macro with storage transformer:validation macro
data modify storage transformer:a0 a3d.rq set from storage transformer:validation rq
