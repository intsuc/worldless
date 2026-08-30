mod common;

use common::context;
use worldless::{
    CommandFeedback, CompiledProgram, ExecutionContext, ExecutionError, ExecutionOutcome,
    MemoryResource, Pack, Position, ResourceKind, Rotation, TickPhase, Vm,
};

const LIMIT: usize = 256;

fn function(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::Function, id, source)
}

fn function_tag(id: &str, values: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::FunctionTag, id, values)
}

fn predicate(id: &str, source: impl Into<String>) -> MemoryResource {
    MemoryResource::new(ResourceKind::Predicate, id, source)
}

fn location(id: &str, x: &str, y: &str, z: &str) -> MemoryResource {
    predicate(
        id,
        r#"{"type":"minecraft:location_check","predicate":{"position":{"x":$X,"y":$Y,"z":$Z}}}"#
            .replace("$X", x)
            .replace("$Y", y)
            .replace("$Z", z),
    )
}

fn compile_program(resources: impl IntoIterator<Item = MemoryResource>) -> CompiledProgram {
    CompiledProgram::from_packs([Pack::memory(resources)]).unwrap()
}

fn compile(resources: impl IntoIterator<Item = MemoryResource>) -> Vm {
    compile_program(resources).create_vm(0)
}

fn outcome(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn execute_at(
    vm: &mut Vm,
    command: &str,
    execution_context: ExecutionContext,
) -> Result<ExecutionOutcome, ExecutionError> {
    vm.execute_command(command, execution_context, LIMIT, drop)
        .into_result()
}

fn run_at(vm: &mut Vm, command: &str, execution_context: ExecutionContext) -> ExecutionOutcome {
    execute_at(vm, command, execution_context).unwrap()
}

fn run(vm: &mut Vm, command: &str) -> ExecutionOutcome {
    run_at(vm, command, context())
}

fn initialize_scores(vm: &mut Vm, holders: &[&str]) {
    assert_eq!(
        run(vm, "scoreboard objectives add state dummy"),
        outcome(true, 1)
    );
    for holder in holders {
        assert_eq!(
            run(vm, &format!("scoreboard players set {holder} state 0")),
            outcome(true, 0)
        );
    }
}

fn score(vm: &mut Vm, holder: &str) -> i32 {
    let outcome = run(vm, &format!("scoreboard players get {holder} state"));
    let ExecutionOutcome::Result {
        success: true,
        value,
    } = outcome
    else {
        panic!("score query did not return a successful result");
    };
    value
}

fn at(x: f64, y: f64, z: f64) -> ExecutionContext {
    ExecutionContext::new(Position::new(x, y, z), Rotation::new(0.0, 0.0))
}

#[test]
fn first_tick_runs_load_then_tick_then_callbacks_scheduled_by_both() {
    let mut vm = compile([
        function(
            "example:load",
            "scoreboard players operation #order state *= #ten state\nscoreboard players add #order state 1\nschedule function example:load_due 1t\n",
        ),
        function(
            "example:tick",
            "scoreboard players operation #order state *= #ten state\nscoreboard players add #order state 2\nschedule function example:tick_due 1t\n",
        ),
        function(
            "example:load_due",
            "scoreboard players operation #order state *= #ten state\nscoreboard players add #order state 3\n",
        ),
        function(
            "example:tick_due",
            "scoreboard players operation #order state *= #ten state\nscoreboard players add #order state 4\n",
        ),
        function_tag("minecraft:load", r#"{"values":["example:load"]}"#),
        function_tag("minecraft:tick", r#"{"values":["example:tick"]}"#),
    ]);
    initialize_scores(&mut vm, &["#order", "#ten"]);
    assert_eq!(
        run(&mut vm, "scoreboard players set #ten state 10"),
        outcome(true, 10)
    );

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#order"), 1_234);
}

#[test]
fn commands_do_not_advance_schedule_time_and_callbacks_fire_on_the_boundary_tick() {
    let mut vm = compile([function(
        "example:due",
        "scoreboard players set #fired state 1\n",
    )]);
    initialize_scores(&mut vm, &["#fired"]);

    assert_eq!(
        run(&mut vm, "schedule function example:due 2t"),
        outcome(true, 2)
    );
    for _ in 0..3 {
        assert_eq!(score(&mut vm, "#fired"), 0);
    }

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#fired"), 0);
    for _ in 0..3 {
        assert_eq!(score(&mut vm, "#fired"), 0);
    }

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#fired"), 1);
}

#[test]
fn append_deduplicates_one_due_tick_while_replace_and_clear_cover_all_due_ticks() {
    let mut vm = compile([function("example:target", "return 1\n")]);

    assert_eq!(
        run(&mut vm, "schedule function example:target 1t append"),
        outcome(true, 1)
    );
    assert_eq!(
        run(&mut vm, "schedule function example:target 1t append"),
        outcome(true, 1)
    );
    assert_eq!(
        run(&mut vm, "schedule function example:target 2t append"),
        outcome(true, 2)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:target"),
        outcome(true, 2)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:target"),
        outcome(false, 0)
    );

    run(&mut vm, "schedule function example:target 1t append");
    run(&mut vm, "schedule function example:target 2t append");
    assert_eq!(
        run(&mut vm, "schedule function example:target 3t replace"),
        outcome(true, 3)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:target"),
        outcome(true, 1)
    );
}

#[test]
fn one_due_callback_can_clear_a_later_callback_at_the_same_tick() {
    let mut vm = compile([
        function(
            "example:a",
            "schedule clear example:b\nscoreboard players set #a state 1\n",
        ),
        function("example:b", "scoreboard players set #b state 1\n"),
    ]);
    initialize_scores(&mut vm, &["#a", "#b"]);
    run(&mut vm, "schedule function example:a 1t append");
    run(&mut vm, "schedule function example:b 1t append");

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#a"), 1);
    assert_eq!(score(&mut vm, "#b"), 0);
}

#[test]
fn invalid_schedules_fail_without_mutating_the_queue_and_missing_tags_are_noops() {
    let mut vm = compile([
        function("example:regular", "return 1\n"),
        function("example:macro", "$return $(value)\n"),
        function(
            "example:tag_plain",
            "scoreboard players set #tag_plain state 1\n",
        ),
        function_tag(
            "example:macro_members",
            r#"{"values":["example:macro","example:tag_plain"]}"#,
        ),
    ]);
    initialize_scores(&mut vm, &["#tag_plain"]);

    assert_eq!(
        run(&mut vm, "schedule function example:unknown 1t"),
        outcome(false, 0)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:unknown"),
        outcome(false, 0)
    );

    assert_eq!(
        run(&mut vm, "schedule function example:regular 2t append"),
        outcome(true, 2)
    );
    assert_eq!(
        run(&mut vm, "schedule function example:regular 0t replace"),
        outcome(false, 0)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:regular"),
        outcome(true, 1)
    );

    assert_eq!(
        run(&mut vm, "schedule function example:macro 1t replace"),
        outcome(false, 0)
    );
    assert_eq!(
        run(&mut vm, "schedule clear example:macro"),
        outcome(false, 0)
    );

    assert_eq!(
        run(&mut vm, "schedule function #example:missing 1t"),
        outcome(true, 1)
    );
    assert_eq!(
        run(
            &mut vm,
            "schedule function #example:macro_members 1t append"
        ),
        outcome(true, 1)
    );
    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#tag_plain"), 1);
}

#[test]
fn function_and_tag_schedules_keep_distinct_ids_and_tag_clear_syntax_is_unsupported() {
    let mut vm = compile([
        function(
            "example:shared",
            "scoreboard players set #function state 1\n",
        ),
        function(
            "example:tag_member",
            "scoreboard players set #tag state 1\n",
        ),
        function_tag("example:shared", r#"{"values":["example:tag_member"]}"#),
    ]);
    initialize_scores(&mut vm, &["#function", "#tag"]);
    assert_eq!(
        run(&mut vm, "schedule function example:shared 1t append"),
        outcome(true, 1)
    );
    assert_eq!(
        run(&mut vm, "schedule function #example:shared 1t append"),
        outcome(true, 1)
    );

    assert!(matches!(
        execute_at(&mut vm, "schedule clear #example:shared", context()),
        Err(ExecutionError::CommandCompilationFailed { .. })
    ));
    assert_eq!(
        run(&mut vm, "schedule clear example:shared"),
        outcome(true, 1)
    );

    assert!(vm.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#function"), 0);
    assert_eq!(score(&mut vm, "#tag"), 1);
}

#[test]
fn scheduled_callbacks_use_the_tick_context_instead_of_the_scheduling_context() {
    let mut vm = compile([
        function(
            "example:observe_context",
            "execute if predicate example:scheduling_context run scoreboard players set #observed state -1\nexecute if predicate example:tick_context run scoreboard players set #observed state 1\n",
        ),
        location("example:scheduling_context", "1", "2", "3"),
        location("example:tick_context", "4", "5", "6"),
    ]);
    initialize_scores(&mut vm, &["#observed"]);

    assert_eq!(
        run_at(
            &mut vm,
            "schedule function example:observe_context 1t",
            at(1.0, 2.0, 3.0),
        ),
        outcome(true, 1)
    );
    assert!(vm.tick(at(4.0, 5.0, 6.0), LIMIT).failures().is_empty());
    assert_eq!(score(&mut vm, "#observed"), 1);
}

#[test]
fn scheduled_callbacks_have_isolated_quotas_and_failures_do_not_stop_later_callbacks() {
    let mut vm = compile([
        function(
            "example:bad",
            "scoreboard players add #bad state 1\nfunction example:bad\n",
        ),
        function("example:good", "scoreboard players add #good state 1\n"),
    ]);
    initialize_scores(&mut vm, &["#bad", "#good"]);
    run(&mut vm, "schedule function example:bad 1t append");
    run(&mut vm, "schedule function example:good 1t append");

    let report = vm.tick(context(), 3);
    assert_eq!(report.failures().len(), 1);
    assert_eq!(report.failures()[0].phase(), TickPhase::Scheduled);
    assert_eq!(report.failures()[0].function(), "example:bad");
    assert_eq!(
        report.failures()[0].error(),
        &ExecutionError::CommandLimitExceeded { limit: 3 }
    );
    assert_eq!(score(&mut vm, "#bad"), 1);
    assert_eq!(score(&mut vm, "#good"), 1);

    assert!(vm.tick(context(), 3).failures().is_empty());
    assert_eq!(score(&mut vm, "#bad"), 1);
    assert_eq!(score(&mut vm, "#good"), 1);
}

#[test]
fn schedule_commands_report_minecraft_results_and_feedback() {
    let mut vm = compile([function("example:target", "return 1\n")]);
    let mut feedback = Vec::new();

    let scheduled = vm
        .execute_command(
            "schedule function example:target 2t",
            context(),
            LIMIT,
            |event| feedback.push(event),
        )
        .into_result()
        .unwrap();
    assert_eq!(scheduled, outcome(true, 2));
    assert!(matches!(
        feedback.as_slice(),
        [CommandFeedback::Success(text)]
            if text.to_string_lossy()
                == "Scheduled function 'example:target' in 2 tick(s) at gametime 2"
    ));

    feedback.clear();
    let cleared = vm
        .execute_command("schedule clear example:target", context(), LIMIT, |event| {
            feedback.push(event)
        })
        .into_result()
        .unwrap();
    assert_eq!(cleared, outcome(true, 1));
    assert!(matches!(
        feedback.as_slice(),
        [CommandFeedback::Success(text)]
            if text.to_string_lossy() == "Removed 1 schedule(s) with ID example:target"
    ));

    feedback.clear();
    let absent = vm
        .execute_command("schedule clear example:target", context(), LIMIT, |event| {
            feedback.push(event)
        })
        .into_result()
        .unwrap();
    assert_eq!(absent, outcome(false, 0));
    assert!(matches!(
        feedback.as_slice(),
        [CommandFeedback::Failure(text)]
            if text.to_string_lossy() == "No schedules with ID example:target"
    ));
}

#[test]
fn vms_from_one_compiled_program_keep_schedule_time_and_queues_separate() {
    let program = compile_program([function(
        "example:due",
        "scoreboard players set #fired state 1\n",
    )]);
    let mut first = program.create_vm(0);
    let mut second = program.create_vm(0);
    initialize_scores(&mut first, &["#fired"]);
    initialize_scores(&mut second, &["#fired"]);

    assert_eq!(
        run(&mut first, "schedule function example:due 2t"),
        outcome(true, 2)
    );
    assert!(first.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut first, "#fired"), 0);

    assert_eq!(
        run(&mut second, "schedule function example:due 2t"),
        outcome(true, 2)
    );
    assert!(second.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut second, "#fired"), 0);
    assert!(second.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut second, "#fired"), 1);
    assert_eq!(score(&mut first, "#fired"), 0);

    assert!(first.tick(context(), LIMIT).failures().is_empty());
    assert_eq!(score(&mut first, "#fired"), 1);
}
