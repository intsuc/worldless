use worldless::{CompileError, ExecutionError, FunctionOutcome, Vm};

const LIMIT: usize = 64;

fn returned(success: bool, value: i32) -> FunctionOutcome {
    FunctionOutcome::Returned { success, value }
}

#[test]
fn scoreboard_commands_report_minecraft_results_and_persist_state() {
    let mut vm = Vm::from_functions([
        (
            "example:create",
            "scoreboard objectives add first dummy\nreturn run scoreboard objectives add values dummy\n",
        ),
        (
            "example:duplicate",
            "return run scoreboard objectives add values dummy\n",
        ),
        (
            "example:set_zero",
            "scoreboard players set #zero values 0\nreturn run scoreboard players get #zero values\n",
        ),
        (
            "example:get_missing_score",
            "return run scoreboard players get #missing values\n",
        ),
        (
            "example:set_missing_objective",
            "return run scoreboard players set #value absent 7\n",
        ),
    ])
    .unwrap();

    assert_eq!(
        vm.execute_function("example:create", LIMIT).unwrap(),
        returned(true, 2)
    );
    assert_eq!(
        vm.execute_function("example:duplicate", LIMIT).unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:set_zero", LIMIT).unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:get_missing_score", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:set_missing_objective", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
}

#[test]
fn execute_store_distinguishes_result_from_success() {
    let mut vm = Vm::from_functions([
        (
            "example:setup",
            "scoreboard objectives add input dummy\nscoreboard objectives add output dummy\n",
        ),
        (
            "example:result",
            "execute store result score #stored output run scoreboard players set #source input 7\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:success",
            "execute store success score #stored output run scoreboard players set #source input 0\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:failed_success",
            "execute store success score #stored output run scoreboard players get #missing input\nreturn run scoreboard players get #stored output\n",
        ),
    ])
    .unwrap();

    assert_eq!(
        vm.execute_function("example:setup", LIMIT).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:result", LIMIT).unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:success", LIMIT).unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:failed_success", LIMIT)
            .unwrap(),
        returned(true, 0)
    );
}

#[test]
fn repeated_stores_run_in_command_order() {
    let mut vm = Vm::from_functions([
        (
            "example:setup",
            "scoreboard objectives add input dummy\nscoreboard objectives add output dummy\n",
        ),
        (
            "example:result_then_success",
            "execute store result score #stored output store success score #stored output run scoreboard players set #source input 7\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:success_then_result",
            "execute store success score #stored output store result score #stored output run scoreboard players set #source input 7\nreturn run scoreboard players get #stored output\n",
        ),
    ])
    .unwrap();

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:result_then_success", LIMIT)
            .unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:success_then_result", LIMIT)
            .unwrap(),
        returned(true, 7)
    );
}

#[test]
fn modifier_order_controls_whether_a_missing_store_target_discards_the_frame() {
    let mut vm = Vm::from_functions([
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #value values 3\n",
        ),
        (
            "example:store_before_return",
            "execute store result score #stored absent run return run scoreboard players set #value values 9\nreturn run scoreboard players get #value values\n",
        ),
        (
            "example:return_before_store",
            "return run execute store result score #stored absent run scoreboard players set #value values 9\nreturn run scoreboard players get #value values\n",
        ),
    ])
    .unwrap();

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:store_before_return", LIMIT)
            .unwrap(),
        returned(true, 3)
    );
    assert_eq!(
        vm.execute_function("example:return_before_store", LIMIT)
            .unwrap(),
        FunctionOutcome::FellThrough
    );
}

#[test]
fn function_results_reach_only_the_callbacks_minecraft_invokes() {
    let mut vm = Vm::from_functions([
        (
            "example:setup",
            "scoreboard objectives add output dummy\n",
        ),
        ("example:return_seven", "return 7\n"),
        (
            "example:store_then_return",
            "execute store result score #stored output run return 7\n",
        ),
        ("example:fallthrough", "# no return\n"),
        (
            "example:normal_return",
            "scoreboard players set #stored output 9\nexecute store result score #stored output run function example:return_seven\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:normal_fallthrough",
            "scoreboard players set #stored output 9\nexecute store result score #stored output run function example:fallthrough\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:normal_missing",
            "scoreboard players set #stored output 9\nexecute store success score #stored output run function example:missing\nreturn run scoreboard players get #stored output\n",
        ),
        (
            "example:returning_return",
            "scoreboard players set #stored output 9\nreturn run execute store result score #stored output run function example:return_seven\n",
        ),
        (
            "example:returning_fallthrough",
            "scoreboard players set #stored output 9\nreturn run execute store result score #stored output run function example:fallthrough\n",
        ),
        (
            "example:returning_missing",
            "scoreboard players set #stored output 9\nreturn run execute store success score #stored output run function example:missing\n",
        ),
        (
            "example:callback_order",
            "scoreboard players set #stored output 9\nreturn run execute store success score #stored output run function example:store_then_return\n",
        ),
        (
            "example:read",
            "return run scoreboard players get #stored output\n",
        ),
    ])
    .unwrap();

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:normal_return", LIMIT).unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:normal_fallthrough", LIMIT)
            .unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:normal_missing", LIMIT)
            .unwrap(),
        returned(true, 0)
    );

    assert_eq!(
        vm.execute_function("example:returning_return", LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read", LIMIT).unwrap(),
        returned(true, 7)
    );

    assert_eq!(
        vm.execute_function("example:returning_fallthrough", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:read", LIMIT).unwrap(),
        returned(true, 9)
    );

    assert_eq!(
        vm.execute_function("example:returning_missing", LIMIT)
            .unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:read", LIMIT).unwrap(),
        returned(true, 0)
    );

    assert_eq!(
        vm.execute_function("example:callback_order", LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read", LIMIT).unwrap(),
        returned(true, 1)
    );
}

#[test]
fn return_run_accepts_every_command_in_the_slice() {
    let mut vm = Vm::from_functions([
        ("example:return", "return run return 4\n"),
        ("example:execute", "execute run return 5\n"),
        (
            "example:nested",
            "return run return run return run return fail\n",
        ),
        (
            "example:nested_frame",
            "return run function example:outer\n",
        ),
        ("example:outer", "function example:child\nreturn 11\n"),
        ("example:child", "return run function example:grandchild\n"),
        ("example:grandchild", "return 7\n"),
    ])
    .unwrap();

    assert_eq!(
        vm.execute_function("example:return", LIMIT).unwrap(),
        returned(true, 4)
    );
    assert_eq!(
        vm.execute_function("example:execute", 2).unwrap(),
        returned(true, 5)
    );
    assert_eq!(
        vm.execute_function("example:nested", 2).unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:nested_frame", LIMIT).unwrap(),
        returned(true, 11)
    );
}

#[test]
fn rejects_commands_outside_the_worldless_scoreboard_slice() {
    for command in [
        "scoreboard players get player values",
        "scoreboard players get @s values",
        "scoreboard players get 00000000-0000-0000-0000-000000000000 values",
        "execute store result score player values run return 1",
        "scoreboard objectives add values trigger",
        "scoreboard objectives add values Dummy",
        "scoreboard objectives add values dummy DisplayName",
        "execute as @s run return 1",
        "execute store result bossbar example:value value run return 1",
    ] {
        assert!(matches!(
            Vm::from_functions([("example:invalid", command)]),
            Err(CompileError::InvalidFunction { .. })
        ));
    }

    assert!(Vm::from_functions([(
        "example:valid",
        "scoreboard objectives add values dummy\nscoreboard players set # values 1\nreturn run scoreboard players get # values\n",
    )])
    .is_ok());
}

#[test]
fn quota_stops_queued_work_but_does_not_roll_back_the_last_executable() {
    let mut vm = Vm::from_functions([
        (
            "example:setup",
            "scoreboard objectives add values dummy\n",
        ),
        (
            "example:set_at_limit",
            "return run scoreboard players set #direct values 7\n",
        ),
        (
            "example:store_before_limit",
            "execute store result score #stored values run scoreboard players set #downstream values 8\n",
        ),
        (
            "example:custom_at_limit",
            "execute store result score #custom_result values store success score #custom_success values run return 6\n",
        ),
        (
            "example:returning_fallthrough",
            "return run function example:empty\n",
        ),
        ("example:empty", "# empty\n"),
        (
            "example:get_direct",
            "return run scoreboard players get #direct values\n",
        ),
        (
            "example:get_stored",
            "return run scoreboard players get #stored values\n",
        ),
        (
            "example:get_downstream",
            "return run scoreboard players get #downstream values\n",
        ),
        (
            "example:get_custom_result",
            "return run scoreboard players get #custom_result values\n",
        ),
        (
            "example:get_custom_success",
            "return run scoreboard players get #custom_success values\n",
        ),
    ])
    .unwrap();

    vm.execute_function("example:setup", LIMIT).unwrap();

    assert_eq!(
        vm.execute_function("example:set_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:get_direct", LIMIT).unwrap(),
        returned(true, 7)
    );

    assert_eq!(
        vm.execute_function("example:store_before_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:get_stored", LIMIT).unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:get_downstream", LIMIT)
            .unwrap(),
        returned(false, 0)
    );

    assert_eq!(
        vm.execute_function("example:store_before_limit", 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:get_stored", LIMIT).unwrap(),
        returned(true, 8)
    );
    assert_eq!(
        vm.execute_function("example:get_downstream", LIMIT)
            .unwrap(),
        returned(true, 8)
    );

    assert_eq!(
        vm.execute_function("example:custom_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:get_custom_result", LIMIT)
            .unwrap(),
        returned(true, 6)
    );
    assert_eq!(
        vm.execute_function("example:get_custom_success", LIMIT)
            .unwrap(),
        returned(true, 1)
    );

    assert_eq!(
        vm.execute_function("example:returning_fallthrough", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:returning_fallthrough", 3)
            .unwrap(),
        returned(false, 0)
    );
}
