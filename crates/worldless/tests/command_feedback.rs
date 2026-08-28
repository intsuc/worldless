mod common;

use common::context;
use worldless::{
    CommandFeedback, ExecutionError, ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 128;

#[derive(Debug, Eq, PartialEq)]
enum RenderedFeedback {
    Success(String),
    Failure(String),
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

fn load(seed: i64, functions: &[(&str, &str)]) -> Vm {
    let resources = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    Vm::from_packs([Pack::memory(resources)], seed).unwrap()
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
fn seed_feedback_keeps_the_full_long_while_the_result_is_narrowed_to_int() {
    let seed = 0x0123_4567_89ab_cdef;
    let mut vm = load(seed, &[]);

    let (outcome, feedback) = execute(&mut vm, "seed");

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: seed as i32,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Success(format!("Seed: [{seed}]"))]
    );
}

#[test]
fn success_and_failure_feedback_are_distinct() {
    let mut vm = load(0, &[]);

    let (created, created_feedback) = execute(&mut vm, "scoreboard objectives add values dummy");
    let (duplicate, duplicate_feedback) =
        execute(&mut vm, "scoreboard objectives add values dummy");

    assert_eq!(
        created.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 1,
        }
    );
    assert_eq!(
        created_feedback,
        [RenderedFeedback::Success(
            "Created new objective [values]".to_owned()
        )]
    );
    assert_eq!(
        duplicate.unwrap(),
        ExecutionOutcome::Result {
            success: false,
            value: 0,
        }
    );
    assert_eq!(
        duplicate_feedback,
        [RenderedFeedback::Failure(
            "An objective already exists by that name".to_owned()
        )]
    );
}

#[test]
fn failures_after_a_forking_condition_do_not_emit_feedback() {
    let mut vm = load(0, &[]);
    for command in [
        "scoreboard objectives add values dummy",
        "scoreboard players set #flag values 1",
    ] {
        execute(&mut vm, command).0.unwrap();
    }

    let (outcome, feedback) = execute(
        &mut vm,
        "execute if score #flag values matches 1 run scoreboard objectives add values dummy",
    );

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: false,
            value: 0,
        }
    );
    assert!(feedback.is_empty());
}

#[test]
fn called_function_bodies_are_silent_but_the_outer_call_reports_in_order() {
    let mut vm = load(
        99,
        &[
            ("example:inner", "seed\nreturn 5\n"),
            ("example:outer", "seed\nfunction example:inner\nreturn 12\n"),
        ],
    );
    let mut feedback = Vec::new();

    let outcome = vm
        .execute_function("example:outer", None, context(), LIMIT, |event| {
            feedback.push(event)
        })
        .unwrap();

    assert_eq!(
        outcome,
        ExecutionOutcome::Result {
            success: true,
            value: 12,
        }
    );
    assert_eq!(
        rendered(feedback),
        [
            RenderedFeedback::Success("Running function example:outer".to_owned()),
            RenderedFeedback::Success("Function example:outer returned 12".to_owned()),
        ]
    );
}

#[test]
fn tag_instantiation_failure_precedes_results_from_the_queued_prefix() {
    let functions = [
        MemoryResource::new(ResourceKind::Function, "example:prefix", "return 7\n"),
        MemoryResource::new(
            ResourceKind::Function,
            "example:bad",
            "$return $(missing)\n",
        ),
        MemoryResource::new(ResourceKind::Function, "example:late", "return 9\n"),
        MemoryResource::new(
            ResourceKind::FunctionTag,
            "example:partial",
            r#"{"values":["example:prefix","example:bad","example:late"]}"#,
        ),
    ];
    let mut vm = Vm::from_packs([Pack::memory(functions)], 0).unwrap();
    let mut feedback = Vec::new();

    let outcome = vm
        .execute_function("#example:partial", None, context(), LIMIT, |event| {
            feedback.push(event)
        })
        .unwrap();

    assert_eq!(
        outcome,
        ExecutionOutcome::Result {
            success: false,
            value: 0,
        }
    );
    assert_eq!(
        rendered(feedback),
        [
            RenderedFeedback::Success(
                "Running functions example:prefix, example:bad, example:late".to_owned(),
            ),
            RenderedFeedback::Failure(
                "Failed to instantiate function example:bad: Missing arguments to function example:bad"
                    .to_owned(),
            ),
            RenderedFeedback::Success("Function example:prefix returned 7".to_owned()),
        ]
    );
}

#[test]
fn execute_store_observes_results_without_consuming_command_feedback() {
    let seed = 0x0123_4567_89ab_cdef;
    let mut vm = load(seed, &[]);
    execute(&mut vm, "scoreboard objectives add values dummy")
        .0
        .unwrap();

    let (stored, feedback) = execute(
        &mut vm,
        "execute store result score #stored values run seed",
    );
    let (read, _) = execute(&mut vm, "scoreboard players get #stored values");

    assert_eq!(
        stored.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: seed as i32,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Success(format!("Seed: [{seed}]"))]
    );
    assert_eq!(
        read.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: seed as i32,
        }
    );
}

#[test]
fn feedback_emitted_before_a_command_limit_error_is_not_lost() {
    let mut vm = load(0, &[("example:main", "return 1\n")]);
    let mut feedback = Vec::new();

    let outcome = vm.execute_function("example:main", None, context(), 1, |event| {
        feedback.push(event)
    });

    assert_eq!(
        outcome,
        Err(ExecutionError::CommandLimitExceeded { limit: 1 })
    );
    assert_eq!(
        rendered(feedback),
        [RenderedFeedback::Success(
            "Running function example:main".to_owned()
        )]
    );
}

#[test]
fn supported_commands_emit_their_console_feedback() {
    let mut vm = load(0, &[]);

    let (_, data) = execute(&mut vm, "data merge storage example:state {value:1}");
    let (_, compute) = execute(&mut vm, "compute default {type:constant,value:7} integer");
    let (random_outcome, random) = execute(&mut vm, "random value 4..5");
    let random_value = match random_outcome.unwrap() {
        ExecutionOutcome::Result {
            success: true,
            value,
        } => value,
        other => panic!("unexpected random outcome: {other:?}"),
    };

    assert_eq!(
        data,
        [RenderedFeedback::Success(
            "Modified storage example:state".to_owned()
        )]
    );
    assert_eq!(
        compute,
        [RenderedFeedback::Success(
            "Number provider returned value 7".to_owned()
        )]
    );
    assert_eq!(
        random,
        [RenderedFeedback::Success(format!(
            "Randomized value: {random_value}"
        ))]
    );
}

#[test]
fn scoreboard_multi_message_feedback_has_a_deterministic_order() {
    let mut vm = load(0, &[]);
    for command in [
        "scoreboard objectives add zeta dummy",
        "scoreboard objectives add alpha dummy",
        "scoreboard players set #holder zeta 2",
        "scoreboard players set #holder alpha 1",
    ] {
        execute(&mut vm, command).0.unwrap();
    }

    let (outcome, feedback) = execute(&mut vm, "scoreboard players list #holder");

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 2,
        }
    );
    assert_eq!(
        feedback,
        [
            RenderedFeedback::Success("#holder has 2 score(s):".to_owned()),
            RenderedFeedback::Success("[alpha]: 1".to_owned()),
            RenderedFeedback::Success("[zeta]: 2".to_owned()),
        ]
    );
}

#[test]
fn scoreboard_set_feedback_uses_the_score_as_its_entity_count_argument() {
    let mut vm = load(0, &[]);
    execute(&mut vm, "scoreboard objectives add values dummy")
        .0
        .unwrap();

    let (outcome, feedback) = execute(&mut vm, "scoreboard players set #zero values 0");

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 0,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Success(
            "Set [values] for 0 entities to 0".to_owned()
        )]
    );
}

#[test]
fn scaled_data_get_feedback_uses_java_half_up_decimal_formatting() {
    let mut vm = load(0, &[]);
    execute(&mut vm, "data merge storage example:state {x:2}")
        .0
        .unwrap();

    let (outcome, feedback) = execute(&mut vm, "data get storage example:state x 2.675");

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 5,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Success(
            "x in storage example:state after scale factor of 2.68 is 5".to_owned()
        )]
    );
}

#[test]
fn data_modify_failure_includes_the_actual_scalar_value() {
    let mut vm = load(0, &[]);
    execute(&mut vm, "data merge storage example:state {x:1}")
        .0
        .unwrap();

    let (outcome, feedback) = execute(
        &mut vm,
        "data modify storage example:state x merge value {added:2}",
    );

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: false,
            value: 0,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Failure(
            "Expected an object: got 1".to_owned()
        )]
    );
}

#[test]
fn data_modify_no_op_feedback_follows_nbt_collection_selection_rules() {
    let mut vm = load(0, &[]);
    for setup in [
        "data merge storage example:scalar {x:1}",
        "data merge storage example:traversal {a:[B;1]}",
        "data merge storage example:index {a:[B;1]}",
    ] {
        execute(&mut vm, setup).0.unwrap();
    }

    for command in [
        "data modify storage example:scalar x.y append value 1",
        "data modify storage example:traversal a[].x append value 1",
        "data modify storage example:index a insert 99 value \"x\"",
    ] {
        let (outcome, feedback) = execute(&mut vm, command);

        assert_eq!(
            outcome.unwrap(),
            ExecutionOutcome::Result {
                success: false,
                value: 0,
            },
            "unexpected outcome for {command}"
        );
        assert_eq!(
            feedback,
            [RenderedFeedback::Failure(
                "Nothing changed. The specified properties already have these values".to_owned(),
            )],
            "unexpected feedback for {command}"
        );
    }
}

#[test]
fn compound_data_get_feedback_matches_minecraft_pretty_snbt() {
    let mut vm = load(0, &[]);
    execute(
        &mut vm,
        "data merge storage example:state {1:2,true:[I;1,2]}",
    )
    .0
    .unwrap();

    let (outcome, feedback) = execute(&mut vm, "data get storage example:state");

    assert_eq!(
        outcome.unwrap(),
        ExecutionOutcome::Result {
            success: true,
            value: 1,
        }
    );
    assert_eq!(
        feedback,
        [RenderedFeedback::Success(
            "Storage example:state has the following contents: {1: 2, true: [I; 1, 2]}".to_owned()
        )]
    );
}
