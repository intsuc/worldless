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
    Vm::from_packs([Pack::memory(functions.into_iter().map(|(id, source)| {
        MemoryResource::new(ResourceKind::Function, id.as_ref(), source.as_ref())
    }))])
}

#[test]
fn add_and_remove_create_scores_and_use_wrapping_i32_arithmetic() {
    let mut vm = compile(&[
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:add_missing",
            "return run scoreboard players add #add_missing values 0\n",
        ),
        (
            "example:add_wrap",
            "scoreboard players set #add_wrap values 2147483647\nreturn run scoreboard players add #add_wrap values 1\n",
        ),
        (
            "example:remove_missing",
            "return run scoreboard players remove #remove_missing values 0\n",
        ),
        (
            "example:remove_wrap",
            "scoreboard players set #remove_wrap values -2147483648\nreturn run scoreboard players remove #remove_wrap values 1\n",
        ),
        (
            "example:missing_objective",
            "return run scoreboard players add #value absent 1\n",
        ),
        (
            "example:read_add_missing",
            "return run scoreboard players get #add_missing values\n",
        ),
        (
            "example:read_add_wrap",
            "return run scoreboard players get #add_wrap values\n",
        ),
        (
            "example:read_remove_missing",
            "return run scoreboard players get #remove_missing values\n",
        ),
        (
            "example:read_remove_wrap",
            "return run scoreboard players get #remove_wrap values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:add_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:add_wrap", context(), LIMIT)
            .unwrap(),
        returned(true, i32::MIN)
    );
    assert_eq!(
        vm.execute_function("example:remove_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:remove_wrap", context(), LIMIT)
            .unwrap(),
        returned(true, i32::MAX)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    for (function, expected) in [
        ("example:read_add_missing", 0),
        ("example:read_add_wrap", i32::MIN),
        ("example:read_remove_missing", 0),
        ("example:read_remove_wrap", i32::MAX),
    ] {
        assert_eq!(
            vm.execute_function(function, context(), LIMIT).unwrap(),
            returned(true, expected),
            "{function}"
        );
    }
}

#[test]
fn operations_match_java_integer_and_scoreboard_semantics() {
    let mut vm = compile(&[
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:assign",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values = #source values\n",
        ),
        (
            "example:add",
            "scoreboard players set #target values 2147483647\nscoreboard players set #source values 1\nreturn run scoreboard players operation #target values += #source values\n",
        ),
        (
            "example:subtract",
            "scoreboard players set #target values -2147483648\nscoreboard players set #source values 1\nreturn run scoreboard players operation #target values -= #source values\n",
        ),
        (
            "example:multiply",
            "scoreboard players set #target values 1073741824\nscoreboard players set #source values 2\nreturn run scoreboard players operation #target values *= #source values\n",
        ),
        (
            "example:divide_negative_dividend",
            "scoreboard players set #target values -7\nscoreboard players set #source values 3\nreturn run scoreboard players operation #target values /= #source values\n",
        ),
        (
            "example:divide_negative_divisor",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values /= #source values\n",
        ),
        (
            "example:divide_overflow",
            "scoreboard players set #target values -2147483648\nscoreboard players set #source values -1\nreturn run scoreboard players operation #target values /= #source values\n",
        ),
        (
            "example:modulo_negative_dividend",
            "scoreboard players set #target values -7\nscoreboard players set #source values 3\nreturn run scoreboard players operation #target values %= #source values\n",
        ),
        (
            "example:modulo_negative_divisor",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values %= #source values\n",
        ),
        (
            "example:modulo_overflow",
            "scoreboard players set #target values -2147483648\nscoreboard players set #source values -1\nreturn run scoreboard players operation #target values %= #source values\n",
        ),
        (
            "example:min",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values < #source values\n",
        ),
        (
            "example:min_keeps_target",
            "scoreboard players set #target values -3\nscoreboard players set #source values 7\nreturn run scoreboard players operation #target values < #source values\n",
        ),
        (
            "example:max",
            "scoreboard players set #target values -3\nscoreboard players set #source values 7\nreturn run scoreboard players operation #target values > #source values\n",
        ),
        (
            "example:max_keeps_target",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values > #source values\n",
        ),
        (
            "example:alias_add",
            "scoreboard players set #same values 6\nreturn run scoreboard players operation #same values += #same values\n",
        ),
        (
            "example:read_target",
            "return run scoreboard players get #target values\n",
        ),
        (
            "example:read_same",
            "return run scoreboard players get #same values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    for (function, expected) in [
        ("example:assign", -3),
        ("example:add", i32::MIN),
        ("example:subtract", i32::MAX),
        ("example:multiply", i32::MIN),
        ("example:divide_negative_dividend", -3),
        ("example:divide_negative_divisor", -3),
        ("example:divide_overflow", i32::MIN),
        ("example:modulo_negative_dividend", 2),
        ("example:modulo_negative_divisor", -2),
        ("example:modulo_overflow", 0),
        ("example:min", -3),
        ("example:min_keeps_target", -3),
        ("example:max", 7),
        ("example:max_keeps_target", 7),
        ("example:alias_add", 12),
    ] {
        assert_eq!(
            vm.execute_function(function, context(), LIMIT).unwrap(),
            returned(true, expected),
            "{function}"
        );
        let reader = if function == "example:alias_add" {
            "example:read_same"
        } else {
            "example:read_target"
        };
        assert_eq!(
            vm.execute_function(reader, context(), LIMIT).unwrap(),
            returned(true, expected),
            "state after {function}"
        );
    }
}

#[test]
fn swap_updates_both_scores_and_handles_an_aliased_score() {
    let mut vm = compile(&[
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:swap",
            "scoreboard players set #target values 7\nscoreboard players set #source values -3\nreturn run scoreboard players operation #target values >< #source values\n",
        ),
        (
            "example:source",
            "return run scoreboard players get #source values\n",
        ),
        (
            "example:alias",
            "scoreboard players set #same values 11\nreturn run scoreboard players operation #same values >< #same values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:swap", context(), LIMIT)
            .unwrap(),
        returned(true, -3)
    );
    assert_eq!(
        vm.execute_function("example:source", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:alias", context(), LIMIT)
            .unwrap(),
        returned(true, 11)
    );
}

#[test]
fn operation_failures_preserve_minecraft_partial_effects() {
    let mut vm = compile(&[
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:divide_by_missing",
            "scoreboard players set #target values 7\nscoreboard players operation #target values /= #missing values\nreturn run scoreboard players get #target values\n",
        ),
        (
            "example:read_missing",
            "return run scoreboard players get #missing values\n",
        ),
        (
            "example:divide_both_missing",
            "scoreboard players operation #zero_target values /= #zero_source values\nreturn run scoreboard players get #zero_target values\n",
        ),
        (
            "example:read_zero_source",
            "return run scoreboard players get #zero_source values\n",
        ),
        (
            "example:missing_source_objective",
            "scoreboard players operation #not_created values += #source absent\nreturn run scoreboard players get #not_created values\n",
        ),
        (
            "example:both_missing",
            "return run scoreboard players operation #left values += #right values\n",
        ),
        (
            "example:read_right",
            "return run scoreboard players get #right values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:divide_by_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:divide_both_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:read_zero_source", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_source_objective", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:both_missing", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:read_right", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn score_comparisons_and_ranges_are_inclusive() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #below values -6\nscoreboard players set #negative values -5\nscoreboard players set #equal values -5\nscoreboard players set #zero values 0\nscoreboard players set #positive values 10\nscoreboard players set #above values 11\nscoreboard players set #min values -2147483648\nscoreboard players set #max values 2147483647\n",
        ),
        (
            "example:equal",
            "return run execute if score #negative values = #equal values\n",
        ),
        (
            "example:less",
            "return run execute if score #negative values < #zero values\n",
        ),
        (
            "example:less_equal",
            "return run execute if score #negative values <= #equal values\n",
        ),
        (
            "example:greater",
            "return run execute if score #positive values > #zero values\n",
        ),
        (
            "example:greater_equal",
            "return run execute if score #positive values >= #positive values\n",
        ),
        (
            "example:exact",
            "return run execute if score #negative values matches -5\n",
        ),
        (
            "example:bounded",
            "return run execute if score #zero values matches -5..10\n",
        ),
        (
            "example:lower_bounded",
            "return run execute if score #positive values matches 10..\n",
        ),
        (
            "example:upper_bounded",
            "return run execute if score #negative values matches ..-5\n",
        ),
        (
            "example:full_i32",
            "execute if score #min values matches -2147483648..2147483647 run execute if score #max values matches -2147483648..2147483647 run return 1\n",
        ),
        (
            "example:false",
            "return run execute if score #negative values > #positive values\n",
        ),
        (
            "example:below_range",
            "return run execute if score #below values matches -5..10\n",
        ),
        (
            "example:above_range",
            "return run execute if score #above values matches -5..10\n",
        ),
        (
            "example:unless",
            "return run execute unless score #negative values > #positive values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    for function in [
        "example:equal",
        "example:less",
        "example:less_equal",
        "example:greater",
        "example:greater_equal",
        "example:exact",
        "example:bounded",
        "example:lower_bounded",
        "example:upper_bounded",
        "example:full_i32",
        "example:unless",
    ] {
        assert_eq!(
            vm.execute_function(function, context(), LIMIT).unwrap(),
            returned(true, 1),
            "{function}"
        );
    }
    assert_eq!(
        vm.execute_function("example:false", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:below_range", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:above_range", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn missing_scores_are_false_but_missing_objectives_abort_both_polarities() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #present values 1\n",
        ),
        (
            "example:missing_score_if",
            "return run execute if score #missing values matches 0\n",
        ),
        (
            "example:missing_score_unless",
            "return run execute unless score #missing values matches 0\n",
        ),
        (
            "example:missing_objective_if",
            "return run execute if score #missing absent matches 0\n",
        ),
        (
            "example:missing_objective_unless",
            "return run execute unless score #missing absent matches 0\n",
        ),
        (
            "example:missing_score_compare_if",
            "return run execute if score #missing values = #present values\n",
        ),
        (
            "example:missing_score_compare_unless",
            "return run execute unless score #missing values = #present values\n",
        ),
        (
            "example:missing_objective_compare_if",
            "return run execute if score #present values = #missing absent\n",
        ),
        (
            "example:missing_objective_compare_unless",
            "return run execute unless score #present values = #missing absent\n",
        ),
        (
            "example:read_missing",
            "return run scoreboard players get #missing values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:missing_score_if", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_score_unless", context(), LIMIT)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective_if", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective_unless", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_score_compare_if", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_score_compare_unless", context(), LIMIT)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective_compare_if", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective_compare_unless", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:read_missing", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn score_conditions_filter_commands_and_preserve_modifier_order() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #left values 1\nscoreboard players set #right values 2\nscoreboard players set #marker values 0\n",
        ),
        (
            "example:passing",
            "return run execute if score #left values < #right values run scoreboard players set #marker values 7\n",
        ),
        (
            "example:condition_before_return",
            "scoreboard players set #marker values 0\nexecute if score #left values = #right values run return run scoreboard players set #marker values 9\nreturn run scoreboard players get #marker values\n",
        ),
        (
            "example:return_before_condition",
            "scoreboard players set #marker values 0\nreturn run execute if score #left values = #right values run scoreboard players set #marker values 9\nreturn 12\n",
        ),
        (
            "example:store_before_false",
            "scoreboard players set #stored values 4\nexecute store result score #stored values if score #left values = #right values run return 9\nreturn run scoreboard players get #stored values\n",
        ),
        (
            "example:missing_objective_before_return",
            "execute unless score #left absent matches 1 run return run scoreboard players set #marker values 9\nreturn 13\n",
        ),
        (
            "example:return_before_missing_objective",
            "return run execute unless score #left absent matches 1 run scoreboard players set #marker values 9\nreturn 13\n",
        ),
        (
            "example:forked_store_failure",
            "return run execute if score #left values < #right values store result score #unused absent run return 9\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:passing", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:condition_before_return", context(), LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:return_before_condition", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:store_before_false", context(), LIMIT)
            .unwrap(),
        returned(true, 4)
    );
    assert_eq!(
        vm.execute_function("example:missing_objective_before_return", context(), LIMIT)
            .unwrap(),
        returned(true, 13)
    );
    assert_eq!(
        vm.execute_function("example:return_before_missing_objective", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:forked_store_failure", context(), LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn inactive_condition_chains_still_charge_later_ordinary_modifiers() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #left values 1\nscoreboard players set #right values 2\n",
        ),
        (
            "example:filtered",
            "execute if score #left values = #right values store result score #unused absent run return 9\nreturn 5\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:filtered", context(), 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:filtered", context(), 4)
            .unwrap(),
        returned(true, 5)
    );
}

#[test]
fn scoreboard_arithmetic_and_conditions_reject_unsupported_syntax() {
    for command in [
        "scoreboard players add player values 1",
        "scoreboard players add #value values -1",
        "scoreboard players add #value values 2147483648",
        "scoreboard players remove @s values 1",
        "scoreboard players operation #target values ^= #source values",
        "scoreboard players operation player values += #source values",
        "scoreboard players operation #target values += player values",
        "execute if score player values matches 0",
        "execute if score #target values = player values",
        "execute if score #target values matches ..",
        "execute if score #target values matches 1..0",
        "execute if score #target values matches +1",
        "execute if score #target values matches 2147483648",
        "execute if score #target values matches 1...2",
    ] {
        assert!(
            matches!(
                load_functions([("example:invalid", command)]),
                Err(LoadError::InvalidFunction { .. })
            ),
            "{command:?}"
        );
    }
}

#[test]
fn operation_cost_is_charged_even_when_execution_fails() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #target values 7\n",
        ),
        (
            "example:divide_zero",
            "return run scoreboard players operation #target values /= #zero values\n",
        ),
        (
            "example:read_target",
            "return run scoreboard players get #target values\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:divide_zero", context(), 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:read_target", context(), LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:divide_zero", context(), 3)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn terminal_score_conditions_have_ordinary_command_cost() {
    let mut vm = compile(&[
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #value values 1\n",
        ),
        (
            "example:condition",
            "return run execute if score #value values matches 1\n",
        ),
    ]);

    vm.execute_function("example:setup", context(), LIMIT)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:condition", context(), 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:condition", context(), 3)
            .unwrap(),
        returned(true, 1)
    );
}
