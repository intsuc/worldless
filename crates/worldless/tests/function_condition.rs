mod common;

use common::context;
use worldless::{
    ExecutionError, FunctionOutcome, LoadError, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 128;

fn returned(success: bool, value: i32) -> FunctionOutcome {
    FunctionOutcome::Returned { success, value }
}

fn compile(functions: &[(&str, &str)]) -> Vm {
    load_functions(functions.iter().copied()).unwrap()
}

fn load_functions<I, N, S>(functions: I) -> Result<Vm, LoadError>
where
    I: IntoIterator<Item = (N, S)>,
    N: AsRef<str>,
    S: AsRef<str>,
{
    Vm::from_packs(
        [Pack::memory(functions.into_iter().map(|(id, source)| {
            MemoryResource::new(ResourceKind::Function, id.as_ref(), source.as_ref())
        }))],
        None,
    )
}

fn load_functions_and_tags<FI, FN, FS, TI, TN, TS>(functions: FI, tags: TI) -> Result<Vm, LoadError>
where
    FI: IntoIterator<Item = (FN, FS)>,
    FN: AsRef<str>,
    FS: AsRef<str>,
    TI: IntoIterator<Item = (TN, TS)>,
    TN: AsRef<str>,
    TS: AsRef<str>,
{
    let functions = functions.into_iter().map(|(id, source)| {
        MemoryResource::new(ResourceKind::Function, id.as_ref(), source.as_ref())
    });
    let tags = tags.into_iter().map(|(id, source)| {
        MemoryResource::new(ResourceKind::FunctionTag, id.as_ref(), source.as_ref())
    });
    Vm::from_packs([Pack::memory(functions.chain(tags))], None)
}

#[test]
fn function_conditions_test_the_return_value_without_using_success() {
    let mut vm = compile(&[
        ("example:nonzero", "return -7\n"),
        ("example:zero", "return 0\n"),
        ("example:failure", "return fail\n"),
        ("example:fallthrough", "# no explicit return\n"),
        (
            "example:if_nonzero",
            "return run execute if function example:nonzero run return 11\n",
        ),
        (
            "example:if_zero",
            "return run execute if function example:zero run return 11\n",
        ),
        (
            "example:unless_nonzero",
            "return run execute unless function example:nonzero run return 12\n",
        ),
        (
            "example:unless_zero",
            "return run execute unless function example:zero run return 12\n",
        ),
        (
            "example:unless_failure",
            "return run execute unless function example:failure run return 13\n",
        ),
        (
            "example:unless_fallthrough",
            "return run execute unless function example:fallthrough run return 14\n",
        ),
        (
            "example:condition_before_return",
            "execute if function example:zero run return run return 15\nreturn 16\n",
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:if_nonzero", context(), LIMIT)
            .unwrap(),
        returned(true, 11)
    );
    assert_eq!(
        vm.execute_function("example:if_zero", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:unless_nonzero", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:unless_zero", context(), LIMIT)
            .unwrap(),
        returned(true, 12)
    );
    assert_eq!(
        vm.execute_function("example:unless_failure", context(), LIMIT)
            .unwrap(),
        returned(true, 13)
    );
    assert_eq!(
        vm.execute_function("example:unless_fallthrough", context(), LIMIT)
            .unwrap(),
        returned(true, 14)
    );
    assert_eq!(
        vm.execute_function("example:condition_before_return", context(), LIMIT)
            .unwrap(),
        returned(true, 16)
    );
}

#[test]
fn only_a_return_from_the_tested_function_produces_a_condition_value() {
    let mut vm = compile(&[
        ("example:child", "return 4\n"),
        ("example:normal_call", "function example:child\n"),
        (
            "example:returning_call",
            "return run function example:child\n",
        ),
        (
            "example:no_callback",
            "return run function example:missing\n",
        ),
        (
            "example:normal_is_zero",
            "return run execute unless function example:normal_call run return 1\n",
        ),
        (
            "example:returning_is_nonzero",
            "return run execute if function example:returning_call run return 2\n",
        ),
        (
            "example:no_callback_is_not_zero",
            "return run execute unless function example:no_callback run return 3\n",
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:normal_is_zero", context(), LIMIT)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:returning_is_nonzero", context(), LIMIT)
            .unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:no_callback_is_not_zero", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn unknown_condition_functions_abort_the_chain_for_both_polarities() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add state dummy\nscoreboard players set #zero state 0\nscoreboard players set #stored state 9\n",
        ),
        (
            "example:condition_before_return",
            "execute unless function example:missing run return run return 9\nreturn 5\n",
        ),
        (
            "example:return_before_condition",
            "return run execute unless function example:missing run return 9\nreturn 5\n",
        ),
        (
            "example:if_before_return",
            "execute if function example:missing run return run return 9\nreturn 6\n",
        ),
        (
            "example:inactive_after_return",
            "return run execute if score #zero state matches 1 unless function example:missing run return 9\n",
        ),
        (
            "example:store_before_unknown",
            "execute store result score #stored state unless function example:missing run return 9\nreturn run scoreboard players get #stored state\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:condition_before_return", context(), LIMIT)
            .unwrap(),
        returned(true, 5)
    );
    assert_eq!(
        vm.execute_function("example:return_before_condition", context(), LIMIT)
            .unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:if_before_return", context(), LIMIT)
            .unwrap(),
        returned(true, 6)
    );
    assert_eq!(
        vm.execute_function("example:inactive_after_return", context(), LIMIT)
            .unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:store_before_unknown", context(), LIMIT)
            .unwrap(),
        returned(true, 9)
    );
}

#[test]
fn tested_functions_keep_side_effects_but_do_not_receive_outer_stores() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add state dummy\nscoreboard players set #stored state 9\nscoreboard players set #evaluations state 0\n",
        ),
        (
            "example:false_with_side_effect",
            "scoreboard players add #evaluations state 1\nreturn 0\n",
        ),
        ("example:nonzero", "return 2\n"),
        (
            "example:filtered_store",
            "execute store result score #stored state if function example:false_with_side_effect run return 7\nreturn run scoreboard players get #stored state\n",
        ),
        (
            "example:passing_store",
            "execute store result score #stored state if function example:nonzero run return 7\n",
        ),
        (
            "example:read_stored",
            "return run scoreboard players get #stored state\n",
        ),
        (
            "example:read_evaluations",
            "return run scoreboard players get #evaluations state\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:filtered_store", context(), LIMIT)
            .unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:read_evaluations", context(), LIMIT)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:passing_store", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read_stored", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
}

#[test]
fn function_conditions_resume_the_remaining_modifier_chain_in_order() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add state dummy\nscoreboard players set #one state 1\nscoreboard players set #zero state 0\nscoreboard players set #calls state 0\n",
        ),
        (
            "example:predicate",
            "scoreboard players add #calls state 1\nreturn 1\n",
        ),
        (
            "example:second_predicate",
            "scoreboard players add #calls state 10\nreturn 1\n",
        ),
        ("example:false_predicate", "return 0\n"),
        (
            "example:function_then_score",
            "return run execute if function example:predicate if score #one state matches 1 run return 8\n",
        ),
        (
            "example:reset_calls",
            "scoreboard players set #calls state 0\n",
        ),
        (
            "example:score_then_function",
            "return run execute if score #one state matches 1 if function example:predicate run return 9\n",
        ),
        (
            "example:inactive_function",
            "execute if score #zero state matches 1 if function example:predicate run return 10\nreturn run scoreboard players get #calls state\n",
        ),
        (
            "example:two_function_conditions",
            "return run execute if function example:predicate if function example:second_predicate run return 11\n",
        ),
        (
            "example:short_circuited_function",
            "execute if function example:false_predicate if function example:second_predicate run return 12\nreturn run scoreboard players get #calls state\n",
        ),
        (
            "example:read_calls",
            "return run scoreboard players get #calls state\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:function_then_score", context(), LIMIT)
            .unwrap(),
        returned(true, 8)
    );
    vm.execute_function("example:reset_calls", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:score_then_function", context(), LIMIT)
            .unwrap(),
        returned(true, 9)
    );
    vm.execute_function("example:reset_calls", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:inactive_function", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:two_function_conditions", context(), LIMIT)
            .unwrap(),
        returned(true, 11)
    );
    assert_eq!(
        vm.execute_function("example:read_calls", context(), LIMIT)
            .unwrap(),
        returned(true, 11)
    );
    vm.execute_function("example:reset_calls", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:short_circuited_function", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn a_function_condition_forks_the_remaining_modifier_chain() {
    let mut vm = compile(&[
        ("example:predicate", "return 1\n"),
        (
            "example:main",
            "return run execute if function example:predicate store result score #stored absent run return 9\n",
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:main", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn function_condition_calls_consume_quota_but_the_modifier_does_not() {
    let mut vm = compile(&[
        ("example:predicate", "return 1\n"),
        (
            "example:condition",
            "return run execute if function example:predicate run return 9\n",
        ),
        (
            "example:recursive",
            "return run execute if function example:recursive run return 1\n",
        ),
    ]);

    assert_eq!(
        vm.execute_function("example:condition", context(), 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:condition", context(), 3)
            .unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:recursive", context(), 10),
        Err(ExecutionError::CommandLimitExceeded { limit: 10 })
    );
}

#[test]
fn quota_failure_preserves_tested_function_side_effects() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add state dummy\nscoreboard players set #calls state 0\n",
        ),
        (
            "example:predicate",
            "scoreboard players add #calls state 1\nreturn 1\n",
        ),
        (
            "example:condition",
            "return run execute if function example:predicate run return 9\n",
        ),
        (
            "example:read_calls",
            "return run scoreboard players get #calls state\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:condition", context(), 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:read_calls", context(), LIMIT)
            .unwrap(),
        returned(true, 1)
    );
}

#[test]
fn function_conditions_reject_terminal_and_invalid_reference_forms() {
    for command in [
        "execute if function example:predicate",
        "execute unless function example:predicate",
        "execute if function ##example:predicates run return 1",
        "execute unless function #Upper:predicates run return 1",
        "execute if function Upper:predicate run return 1",
    ] {
        assert!(
            matches!(
                load_functions([("example:invalid", command)]),
                Err(LoadError::InvalidFunction { .. })
            ),
            "{command:?}"
        );
    }

    assert!(
        load_functions_and_tags(
            [
                (
                    "example:valid",
                    "execute if function #example:predicates run return 1\n",
                ),
                ("example:predicate", "return 1\n"),
            ],
            [("example:predicates", r#"{"values":["example:predicate"]}"#)],
        )
        .is_ok()
    );
}
