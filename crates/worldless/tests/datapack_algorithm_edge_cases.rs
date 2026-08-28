mod common;

use common::context;
use worldless::{CompiledProgram, ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 16_384;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn function(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::Function, id, source)
}

fn execute(vm: &mut Vm, command: &str) -> ExecutionOutcome {
    vm.execute_command(command, context(), LIMIT, drop)
        .into_result()
        .unwrap()
}

fn vm(pack: Pack) -> Vm {
    CompiledProgram::from_packs([pack]).unwrap().create_vm(0)
}

#[test]
fn deep_wildcard_filter_removal_counts_matches_and_preserves_siblings() {
    let mut vm = vm(Pack::memory(std::iter::empty::<MemoryResource>()));
    assert_eq!(
        execute(
            &mut vm,
            "data merge storage heap:gc {_: [{_: [{_: [{_:{_count:1610612640},value:1},{_:{_count:1},value:2}]},{_: [{_:{_count:1610612640},value:3}]}]},{_: [{_: [{_:{_count:1610612640},value:4},{_:{_count:2},value:5}]}]}]}",
        ),
        returned(true, 1)
    );

    let dead = "_[]._[]._[{_:{_count:1610612640}}]";
    assert_eq!(
        execute(&mut vm, &format!("execute if data storage heap:gc {dead}")),
        returned(true, 3)
    );
    assert_eq!(
        execute(&mut vm, &format!("data remove storage heap:gc {dead}")),
        returned(true, 3)
    );
    assert_eq!(
        execute(&mut vm, &format!("execute if data storage heap:gc {dead}")),
        returned(false, 0)
    );
    assert_eq!(
        execute(&mut vm, "execute if data storage heap:gc _[]._[]._[]",),
        returned(true, 2)
    );
    for value in [2, 5] {
        assert_eq!(
            execute(
                &mut vm,
                &format!("execute if data storage heap:gc _[]._[]._[{{value:{value}}}]")
            ),
            returned(true, 1),
            "surviving value {value}"
        );
    }
}

#[test]
fn mixed_score_and_storage_stores_receive_the_same_command_result() {
    let mut vm = vm(Pack::memory(std::iter::empty::<MemoryResource>()));
    assert_eq!(
        execute(&mut vm, "scoreboard objectives add heap dummy"),
        returned(true, 1)
    );
    assert_eq!(
        execute(&mut vm, "data merge storage heap:source {value:37}",),
        returned(true, 1)
    );

    assert_eq!(
        execute(
            &mut vm,
            "execute store result score #copy heap store result storage heap:stored value int 1 run data get storage heap:source value",
        ),
        returned(true, 37)
    );
    assert_eq!(
        execute(&mut vm, "scoreboard players get #copy heap"),
        returned(true, 37)
    );
    assert_eq!(
        execute(&mut vm, "execute if data storage heap:stored {value:37}",),
        returned(true, 1)
    );
}

#[test]
fn in_memory_functions_admit_a_trailing_slash_identifier() {
    let mut vm = vm(Pack::memory([
        function("example:leaf/", "return 23\n"),
        function("example:caller", "return run function example:leaf/\n"),
    ]));

    for function in ["example:leaf/", "example:caller"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .into_result()
                .unwrap(),
            returned(true, 23),
            "{function}"
        );
    }
}
