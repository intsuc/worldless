mod common;

use common::context;
use worldless::{
    ExecutionError, ExecutionOutcome, FunctionArguments, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 128;

fn result(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn compile(functions: &[(&str, &str)], tags: &[(&str, &str)]) -> Vm {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let tags = tags
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::FunctionTag, *id, *source));
    Vm::from_packs([Pack::memory(functions.chain(tags))], None).unwrap()
}

#[test]
fn function_arguments_require_one_complete_compound() {
    FunctionArguments::from_snbt(
        r#"  {byte:1b,short:2s,int:3,long:4L,float:.5f,double:.25d,string:"x",list:[1,2],nested:{value:7}}  "#,
    )
    .unwrap();

    for invalid in ["", "7", "[]", "{value:7} trailing", "{value:7}{}"] {
        assert!(
            FunctionArguments::from_snbt(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn execute_function_accepts_plain_and_macro_arguments_without_defaulting_absence() {
    let mut vm = compile(
        &[
            ("example:plain", "return 3\n"),
            ("example:macro", "$return $(value)\n"),
        ],
        &[],
    );
    let unused = FunctionArguments::from_snbt("{unused:99}").unwrap();
    let value = FunctionArguments::from_snbt("{value:7b,unused:99}").unwrap();
    let empty = FunctionArguments::from_snbt("{}").unwrap();

    assert_eq!(
        vm.execute_function("example:plain", Some(&unused), context(), LIMIT)
            .unwrap(),
        result(true, 3)
    );
    assert_eq!(
        vm.execute_function("example:macro", Some(&value), context(), LIMIT)
            .unwrap(),
        result(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:macro", None, context(), LIMIT)
            .unwrap(),
        result(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:macro", Some(&empty), context(), LIMIT)
            .unwrap(),
        result(false, 0)
    );
}

#[test]
fn invalid_function_references_are_rejected_before_execution() {
    let mut vm = compile(&[("example:plain", "return 3\n")], &[]);

    assert_eq!(
        vm.execute_function("Example:plain", None, context(), LIMIT),
        Err(ExecutionError::InvalidFunctionReference {
            input: "Example:plain".to_owned(),
        })
    );
}

#[test]
fn execute_function_tags_use_function_command_results() {
    let mut vm = compile(
        &[
            ("example:max", "return 2147483647\n"),
            ("example:one", "return 1\n"),
            ("example:fallthrough_a", "# no result\n"),
            ("example:fallthrough_b", "# still no result\n"),
        ],
        &[
            (
                "example:wrapping",
                r#"{"values":["example:max","example:one"]}"#,
            ),
            (
                "example:fallthrough",
                r#"{"values":["example:fallthrough_a","example:fallthrough_b"]}"#,
            ),
            ("example:empty", r#"{"values":[]}"#),
        ],
    );

    assert_eq!(
        vm.execute_function("#example:wrapping", None, context(), LIMIT)
            .unwrap(),
        result(true, i32::MIN)
    );
    assert_eq!(
        vm.execute_function("#example:fallthrough", None, context(), LIMIT)
            .unwrap(),
        ExecutionOutcome::NoResult
    );
    for reference in ["example:missing", "#example:missing", "#example:empty"] {
        assert_eq!(
            vm.execute_function(reference, None, context(), LIMIT)
                .unwrap(),
            result(false, 0),
            "{reference}"
        );
    }
}

#[test]
fn tag_macros_share_arguments_and_instantiation_failure_keeps_only_the_prefix() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #prefix state 0\nscoreboard players set #late state 0\n",
            ),
            ("example:first", "$return $(value)\n"),
            ("example:second", "$return $(value)\n"),
            (
                "example:prefix",
                "scoreboard players add #prefix state 1\nreturn 4\n",
            ),
            ("example:bad", "$return $(missing)\n"),
            (
                "example:late",
                "scoreboard players add #late state 1\nreturn 8\n",
            ),
        ],
        &[
            (
                "example:shared",
                r#"{"values":["example:first","example:second"]}"#,
            ),
            (
                "example:failing",
                r#"{"values":["example:prefix","example:bad","example:late"]}"#,
            ),
        ],
    );
    let shared = FunctionArguments::from_snbt("{value:6}").unwrap();
    let incomplete = FunctionArguments::from_snbt("{other:1}").unwrap();

    assert_eq!(
        vm.execute_function("#example:shared", Some(&shared), context(), LIMIT)
            .unwrap(),
        result(true, 12)
    );
    assert_eq!(
        vm.execute_function("example:setup", None, context(), LIMIT)
            .unwrap(),
        ExecutionOutcome::NoResult
    );
    assert_eq!(
        vm.execute_function("#example:failing", Some(&incomplete), context(), LIMIT)
            .unwrap(),
        result(false, 0)
    );
    assert_eq!(
        vm.execute_command("scoreboard players get #prefix state", context(), LIMIT)
            .unwrap(),
        result(true, 1)
    );
    assert_eq!(
        vm.execute_command("scoreboard players get #late state", context(), LIMIT)
            .unwrap(),
        result(true, 0)
    );
}

#[test]
fn execute_command_reports_results_and_no_result_for_each_entry_kind() {
    let mut vm = compile(
        &[
            ("example:plain", "return 4\n"),
            ("example:macro", "$return $(value)\n"),
            ("example:one", "return 1\n"),
            ("example:two", "return 2\n"),
        ],
        &[(
            "example:both",
            r#"{"values":["example:one","example:two"]}"#,
        )],
    );

    assert_eq!(
        vm.execute_command("scoreboard objectives list", context(), LIMIT)
            .unwrap(),
        result(true, 0)
    );
    assert_eq!(
        vm.execute_command("scoreboard players get #missing values", context(), LIMIT)
            .unwrap(),
        result(false, 0)
    );
    assert_eq!(
        vm.execute_command(
            "execute if score #missing values matches 1 run return 9",
            context(),
            LIMIT,
        )
        .unwrap(),
        ExecutionOutcome::NoResult
    );
    assert_eq!(
        vm.execute_command(
            "return run execute if score #missing values matches 1 run return 9",
            context(),
            LIMIT,
        )
        .unwrap(),
        result(false, 0)
    );
    assert_eq!(
        vm.execute_command("return 7", context(), LIMIT).unwrap(),
        result(true, 7)
    );
    assert_eq!(
        vm.execute_command("/return 8", context(), LIMIT).unwrap(),
        result(true, 8)
    );
    assert_eq!(
        vm.execute_command("function example:plain", context(), LIMIT)
            .unwrap(),
        result(true, 4)
    );
    assert_eq!(
        vm.execute_command("function #example:both", context(), LIMIT)
            .unwrap(),
        result(true, 3)
    );
    assert_eq!(
        vm.execute_command("function example:macro {value:11}", context(), LIMIT)
            .unwrap(),
        result(true, 11)
    );
}

#[test]
fn invalid_and_unsupported_commands_are_rejected_before_side_effects() {
    for invalid in [
        "//scoreboard objectives add marker dummy",
        "scoreboard objectives add marker dummy trailing",
        "scoreboard objectives add marker trigger",
    ] {
        let mut vm = compile(&[], &[]);

        assert!(
            vm.execute_command(invalid, context(), LIMIT).is_err(),
            "accepted {invalid:?}"
        );
        assert_eq!(
            vm.execute_command("scoreboard objectives add marker dummy", context(), LIMIT)
                .unwrap(),
            result(true, 1),
            "{invalid:?} changed state before failing"
        );
    }
}

#[test]
fn direct_command_quota_does_not_include_a_synthetic_function_call() {
    let mut vm = compile(&[], &[]);

    assert_eq!(
        vm.execute_command("scoreboard objectives list", context(), 2)
            .unwrap(),
        result(true, 0)
    );
}
