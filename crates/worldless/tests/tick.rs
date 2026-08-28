mod common;

use common::context;
use worldless::{
    ExecutionError, ExecutionOutcome, MemoryResource, Pack, ResourceKind, TickPhase, Vm,
};

const LIMIT: usize = 256;

fn function(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::Function, id, source)
}

fn function_tag(id: &str, values: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::FunctionTag, id, values)
}

fn compile(resources: impl IntoIterator<Item = MemoryResource>) -> Vm {
    Vm::from_packs([Pack::memory(resources)], 0).unwrap()
}

fn score(vm: &mut Vm, holder: &str) -> i32 {
    let ExecutionOutcome::Result {
        success: true,
        value,
    } = vm
        .execute_command(
            &format!("scoreboard players get {holder} state"),
            context(),
            LIMIT,
            drop,
        )
        .unwrap()
    else {
        panic!("score query did not return a result");
    };
    value
}

#[test]
fn first_normal_tick_runs_load_before_tick_and_later_ticks_skip_load() {
    let mut vm = compile([
        function(
            "example:load_setup",
            "scoreboard objectives add state dummy\nscoreboard players set #load state 1\nscoreboard players set #tick state 0\n",
        ),
        function(
            "example:load_after",
            "scoreboard players add #load state 10\n",
        ),
        function(
            "example:tick_first",
            "scoreboard players add #tick state 1\n",
        ),
        function(
            "example:tick_after",
            "scoreboard players add #tick state 10\n",
        ),
        function_tag(
            "minecraft:load",
            r#"{"values":["example:load_setup","example:load_after"]}"#,
        ),
        function_tag(
            "minecraft:tick",
            r#"{"values":["example:tick_first","example:tick_after"]}"#,
        ),
    ]);

    assert_eq!(
        vm.execute_command("scoreboard objectives list", context(), LIMIT, drop)
            .unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 0,
        }
    );
    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#load"), 11);
    assert_eq!(score(&mut vm, "#tick"), 11);

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#load"), 11);
    assert_eq!(score(&mut vm, "#tick"), 22);
}

#[test]
fn automatic_members_have_isolated_queues_and_failures_do_not_stop_the_tick() {
    let mut vm = compile([
        function(
            "example:macro",
            "$scoreboard players add #macro state $(amount)\n",
        ),
        function(
            "example:bad_load",
            "scoreboard players add #bad_load state 1\nfunction example:bad_load\n",
        ),
        function(
            "example:good_load",
            "scoreboard players add #good_load state 1\n",
        ),
        function("example:return_load", "return 7\n"),
        function(
            "example:bad_tick",
            "scoreboard players add #bad_tick state 1\nfunction example:bad_tick\n",
        ),
        function(
            "example:good_tick",
            "scoreboard players add #good_tick state 1\n",
        ),
        function("example:return_tick", "return 8\n"),
        function_tag(
            "minecraft:load",
            r#"{"values":["example:macro","example:bad_load","example:return_load","example:good_load"]}"#,
        ),
        function_tag(
            "minecraft:tick",
            r#"{"values":["example:macro","example:bad_tick","example:return_tick","example:good_tick"]}"#,
        ),
    ]);
    vm.execute_command(
        "scoreboard objectives add state dummy",
        context(),
        LIMIT,
        drop,
    )
    .unwrap();

    let first = vm.tick(context(), 3);
    assert_eq!(first.failures().len(), 2);
    assert_eq!(first.failures()[0].phase(), TickPhase::Load);
    assert_eq!(first.failures()[0].function(), "example:bad_load");
    assert_eq!(
        first.failures()[0].error(),
        &ExecutionError::CommandLimitExceeded { limit: 3 }
    );
    assert_eq!(first.failures()[1].phase(), TickPhase::Tick);
    assert_eq!(first.failures()[1].function(), "example:bad_tick");
    assert_eq!(
        first.failures()[1].error(),
        &ExecutionError::CommandLimitExceeded { limit: 3 }
    );
    assert_eq!(score(&mut vm, "#bad_load"), 1);
    assert_eq!(score(&mut vm, "#good_load"), 1);
    assert_eq!(score(&mut vm, "#bad_tick"), 1);
    assert_eq!(score(&mut vm, "#good_tick"), 1);

    let second = vm.tick(context(), 3);
    assert_eq!(second.failures().len(), 1);
    assert_eq!(second.failures()[0].phase(), TickPhase::Tick);
    assert_eq!(second.failures()[0].function(), "example:bad_tick");
    assert_eq!(score(&mut vm, "#bad_load"), 1);
    assert_eq!(score(&mut vm, "#good_load"), 1);
    assert_eq!(score(&mut vm, "#bad_tick"), 2);
    assert_eq!(score(&mut vm, "#good_tick"), 2);
}
