mod common;

use common::context;
use worldless::{
    CommandFeedback, ExecutionError, ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 64;

#[derive(Debug, Eq, PartialEq)]
enum RenderedFeedback {
    Success(String),
    Failure(String),
}

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn rendered(feedback: Vec<CommandFeedback>) -> Vec<RenderedFeedback> {
    feedback
        .into_iter()
        .map(|feedback| match feedback {
            CommandFeedback::Success(text) => RenderedFeedback::Success(text.to_string_lossy()),
            CommandFeedback::Failure(text) => RenderedFeedback::Failure(text.to_string_lossy()),
        })
        .collect()
}

fn load(functions: &[(&str, &str)]) -> Vm {
    let resources = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    Vm::from_packs([Pack::memory(resources)], 0).unwrap()
}

fn execute(
    vm: &mut Vm,
    command: &str,
) -> (
    Result<ExecutionOutcome, ExecutionError>,
    Vec<RenderedFeedback>,
) {
    let mut feedback = Vec::new();
    let outcome = vm.execute_command(command, context(), LIMIT, |event| feedback.push(event));
    (outcome, rendered(feedback))
}

#[test]
fn lifecycle_commands_report_results_and_feedback() {
    let mut vm = load(&[]);

    let (created, created_feedback) = execute(&mut vm, "stopwatch create example:clock");
    assert_eq!(created.unwrap(), returned(true, 1));
    assert_eq!(
        created_feedback,
        [RenderedFeedback::Success(
            "Created stopwatch 'example:clock'".to_owned()
        )]
    );

    let (duplicate, duplicate_feedback) = execute(&mut vm, "stopwatch create example:clock");
    assert_eq!(duplicate.unwrap(), returned(false, 0));
    assert_eq!(
        duplicate_feedback,
        [RenderedFeedback::Failure(
            "Stopwatch 'example:clock' already exists".to_owned()
        )]
    );

    let (queried, query_feedback) = execute(&mut vm, "stopwatch query example:clock 0");
    assert_eq!(queried.unwrap(), returned(true, 0));
    let [RenderedFeedback::Success(query_feedback)] = query_feedback.as_slice() else {
        panic!("unexpected query feedback: {query_feedback:?}");
    };
    let elapsed = query_feedback
        .strip_prefix("Stopwatch 'example:clock' has run for ")
        .and_then(|value| value.strip_suffix('s'))
        .and_then(|value| value.parse::<f64>().ok())
        .expect("query feedback must contain the elapsed seconds");
    assert!(elapsed >= 0.0);

    let (restarted, restarted_feedback) = execute(&mut vm, "stopwatch restart example:clock");
    assert_eq!(restarted.unwrap(), returned(true, 1));
    assert_eq!(
        restarted_feedback,
        [RenderedFeedback::Success(
            "Restarted stopwatch 'example:clock'".to_owned()
        )]
    );

    let (removed, removed_feedback) = execute(&mut vm, "stopwatch remove example:clock");
    assert_eq!(removed.unwrap(), returned(true, 1));
    assert_eq!(
        removed_feedback,
        [RenderedFeedback::Success(
            "Removed stopwatch 'example:clock'".to_owned()
        )]
    );

    for command in [
        "stopwatch query example:clock 0",
        "stopwatch restart example:clock",
        "stopwatch remove example:clock",
    ] {
        let (outcome, feedback) = execute(&mut vm, command);
        assert_eq!(outcome.unwrap(), returned(false, 0), "{command}");
        assert_eq!(
            feedback,
            [RenderedFeedback::Failure(
                "Stopwatch 'example:clock' does not exist".to_owned()
            )],
            "{command}"
        );
    }
}

#[test]
fn stopwatch_conditions_use_nonnegative_elapsed_seconds() {
    let mut vm = load(&[]);
    execute(&mut vm, "stopwatch create example:clock")
        .0
        .unwrap();

    assert_eq!(
        execute(
            &mut vm,
            "execute if stopwatch example:clock 0.. run return 7",
        )
        .0
        .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        execute(
            &mut vm,
            "execute unless stopwatch example:clock 0.. run return 7",
        )
        .0
        .unwrap(),
        ExecutionOutcome::NoResult
    );
}

#[test]
fn terminal_conditions_report_results_and_missing_is_not_negated() {
    let mut vm = load(&[]);
    execute(&mut vm, "stopwatch create example:clock")
        .0
        .unwrap();

    let (passed, passed_feedback) = execute(&mut vm, "execute if stopwatch example:clock 0..");
    assert_eq!(passed.unwrap(), returned(true, 1));
    assert_eq!(
        passed_feedback,
        [RenderedFeedback::Success("Test passed".to_owned())]
    );

    let (failed, failed_feedback) = execute(&mut vm, "execute unless stopwatch example:clock 0..");
    assert_eq!(failed.unwrap(), returned(false, 0));
    assert_eq!(
        failed_feedback,
        [RenderedFeedback::Failure("Test failed".to_owned())]
    );

    for command in [
        "execute if stopwatch example:missing 0..",
        "execute unless stopwatch example:missing 0..",
    ] {
        let (outcome, feedback) = execute(&mut vm, command);
        assert_eq!(outcome.unwrap(), returned(false, 0), "{command}");
        assert_eq!(
            feedback,
            [RenderedFeedback::Failure(
                "Stopwatch 'example:missing' does not exist".to_owned()
            )],
            "{command}"
        );
    }

    for command in [
        "execute if stopwatch example:missing 0.. run return 7",
        "execute unless stopwatch example:missing 0.. run return 7",
    ] {
        let (outcome, feedback) = execute(&mut vm, command);
        assert_eq!(outcome.unwrap(), ExecutionOutcome::NoResult, "{command}");
        assert!(feedback.is_empty(), "{command}: {feedback:?}");
    }

    let (returned_missing, returned_missing_feedback) = execute(
        &mut vm,
        "return run execute if stopwatch example:missing 0.. run return 7",
    );
    assert_eq!(returned_missing.unwrap(), returned(false, 0));
    assert!(returned_missing_feedback.is_empty());
}

#[test]
fn a_missing_forked_condition_does_not_notify_prior_stores() {
    let mut vm = load(&[]);
    for command in [
        "scoreboard objectives add values dummy",
        "scoreboard players set #result values 9",
        "scoreboard players set #success values 9",
    ] {
        execute(&mut vm, command).0.unwrap();
    }

    let (outcome, feedback) = execute(
        &mut vm,
        "execute store result score #result values store success score #success values if stopwatch example:missing 0.. run return 7",
    );
    assert_eq!(outcome.unwrap(), ExecutionOutcome::NoResult);
    assert!(feedback.is_empty());
    for holder in ["#result", "#success"] {
        assert_eq!(
            execute(&mut vm, &format!("scoreboard players get {holder} values"))
                .0
                .unwrap(),
            returned(true, 9),
            "{holder}"
        );
    }
}

#[test]
fn scaled_query_integrates_with_result_and_success_stores() {
    let mut vm = load(&[]);
    for command in [
        "scoreboard objectives add values dummy",
        "stopwatch create example:clock",
    ] {
        execute(&mut vm, command).0.unwrap();
    }

    assert_eq!(
        execute(
            &mut vm,
            "execute store result score #result values store success score #success values run stopwatch query example:clock 0",
        )
        .0
        .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        execute(&mut vm, "scoreboard players get #result values")
            .0
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        execute(&mut vm, "scoreboard players get #success values")
            .0
            .unwrap(),
        returned(true, 1)
    );
}

#[test]
fn state_persists_across_invocations_and_query_can_return_zero() {
    let mut vm = load(&[(
        "example:query",
        "return run stopwatch query example:persistent 0\n",
    )]);

    assert_eq!(
        execute(&mut vm, "stopwatch create example:persistent")
            .0
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:query", None, context(), LIMIT, drop)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn quota_errors_do_not_roll_back_an_executed_stopwatch_command() {
    let mut vm = load(&[(
        "example:create_at_limit",
        "return run stopwatch create example:quota\n",
    )]);

    assert_eq!(
        vm.execute_function("example:create_at_limit", None, context(), 2, drop),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        execute(&mut vm, "stopwatch query example:quota 0")
            .0
            .unwrap(),
        returned(true, 0)
    );
}
