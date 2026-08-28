mod common;

use common::context;
use worldless::{
    CompiledProgram, CompoundTag, ExecutionError, ExecutionOutcome, MemoryResource, Pack,
    ResourceKind,
};

const LIMIT: usize = 128;

fn program() -> CompiledProgram {
    CompiledProgram::from_packs([Pack::memory([
        MemoryResource::new(
            ResourceKind::Function,
            "example:read",
            "return run data get storage example:input value\n",
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "example:macro",
            "$return $(value)\n",
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "example:over_limit",
            "execute store result storage example:result value int 1 store success storage example:result success byte 1 run return 6\n",
        ),
    ])])
    .unwrap()
}

#[test]
fn compiled_program_creates_vms_with_independent_state() {
    let program = program();
    let mut first = program.create_vm(7);
    let second = program.clone().create_vm(7);
    let input = CompoundTag::from_snbt(r#"{value:7,text:"\uD800"}"#).unwrap();

    first.set_storage("example:input", input.clone()).unwrap();
    assert_eq!(first.storage("example:input").unwrap(), Some(&input));
    assert_eq!(second.storage("example:input").unwrap(), None);
    assert!(input.to_compact_snbt_utf16().contains(&0xd800));

    let error = first
        .set_storage(
            "Invalid:input",
            CompoundTag::from_snbt("{other:1}").unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.input(), "Invalid:input");
    assert_eq!(first.storage("example:input").unwrap(), Some(&input));

    first
        .set_storage("example:input", CompoundTag::from_snbt("{}").unwrap())
        .unwrap();
    assert_eq!(first.storage("example:input").unwrap(), None);
}

#[test]
fn execution_reports_preserve_results_and_exact_quota_on_failure() {
    let program = program();
    let mut vm = program.create_vm(0);
    vm.set_storage(
        "example:input",
        CompoundTag::from_snbt("{value:7}").unwrap(),
    )
    .unwrap();

    let completed = vm.execute_function("example:read", None, context(), LIMIT, drop);
    assert_eq!(completed.quota_used(), 2);
    assert_eq!(
        completed.into_result(),
        Ok(ExecutionOutcome::Result {
            success: true,
            value: 7,
        })
    );

    let arguments = CompoundTag::from_snbt("{value:9}").unwrap();
    let macro_report =
        vm.execute_function("example:macro", Some(&arguments), context(), LIMIT, drop);
    assert_eq!(
        macro_report.result(),
        Ok(ExecutionOutcome::Result {
            success: true,
            value: 9,
        })
    );

    let invalid = vm.execute_function("Invalid:read", None, context(), LIMIT, drop);
    assert_eq!(invalid.quota_used(), 0);
    assert_eq!(
        invalid.into_result(),
        Err(ExecutionError::InvalidFunctionReference {
            input: "Invalid:read".to_owned(),
        })
    );

    let invalid_command = vm.execute_command("scoreboard", context(), LIMIT, drop);
    assert_eq!(invalid_command.quota_used(), 0);
    assert!(matches!(
        invalid_command.into_result(),
        Err(ExecutionError::CommandCompilationFailed { .. })
    ));

    let limited = vm.execute_function("example:over_limit", None, context(), 2, drop);
    assert_eq!(limited.quota_used(), 3);
    assert_eq!(
        limited.into_result(),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
}
