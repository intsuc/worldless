mod common;

use common::context;
use worldless::{CompiledProgram, ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 1_024;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn compile(functions: &[(&str, &str)]) -> Vm {
    CompiledProgram::from_packs([Pack::memory(
        functions
            .iter()
            .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source)),
    )])
    .map(|program| program.create_vm(0))
    .unwrap()
}

#[test]
fn scaled_long_split_recovers_both_signed_i32_halves() {
    let mut vm = compile(&[
        (
            "example:init",
            "scoreboard objectives add edge dummy\ndata modify storage example:long wrapper set value [I;0]\n",
        ),
        (
            "example:split",
            r#"data modify storage example:long wrapper[0] set from storage example:long source
execute store result score #low edge run data get storage example:long wrapper[0] 1

execute store result score #high edge run data get storage example:long source 0.00000000023283064365386963
execute store result score #complement edge run data get storage example:long source -0.00000000023283064365386963

scoreboard players operation #correction edge = #high edge
scoreboard players operation #correction edge > #complement edge

execute if score #low edge matches 0.. run return 0
execute if score #correction edge matches 0..2097151 run return 0
execute if score #low edge matches -512.. if score #correction edge matches 1073741824..2147483646 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -256.. if score #correction edge matches 536870912..1073741823 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -128.. if score #correction edge matches 268435456..536870911 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -64.. if score #correction edge matches 134217728..268435455 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -32.. if score #correction edge matches 67108864..134217727 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -16.. if score #correction edge matches 33554432..67108863 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -8.. if score #correction edge matches 16777216..33554431 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -4.. if score #correction edge matches 8388608..16777215 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -2.. if score #correction edge matches 4194304..8388607 run return run scoreboard players remove #high edge 1
execute if score #low edge matches -1.. if score #correction edge matches 2097152..4194303 run return run scoreboard players remove #high edge 1
execute if score #correction edge matches 2147483647.. if score #low edge matches -512.. if score #complement edge matches -2147483647.. run scoreboard players remove #high edge 1
"#,
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:init", None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        ExecutionOutcome::NoResult
    );

    for value in [
        1_782_934_792_843_521_910_i64,
        -7_391_011_204_884_992_123,
        i64::MIN,
        i64::MAX,
        0,
        -1,
        0x0000_0000_ffff_ffff,
        -0x0000_0001_0000_0000,
    ] {
        vm.execute_command(
            &format!("data modify storage example:long source set value {value}L"),
            context(),
            LIMIT,
            drop,
        )
        .into_result()
        .unwrap();
        vm.execute_function("example:split", None, context(), LIMIT, drop)
            .into_result()
            .unwrap();

        assert_eq!(
            vm.execute_command("scoreboard players get #high edge", context(), LIMIT, drop,)
                .into_result()
                .unwrap(),
            returned(true, (value >> 32) as i32),
            "high half of {value}",
        );
        assert_eq!(
            vm.execute_command("scoreboard players get #low edge", context(), LIMIT, drop,)
                .into_result()
                .unwrap(),
            returned(true, value as i32),
            "low half of {value}",
        );
    }
}
