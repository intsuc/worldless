use worldless::{
    ExecutionError, FunctionOutcome, LoadError, MemoryResource, Pack, ResourceKind, ResourceOrigin,
    Vm,
};

const LIMIT: usize = 128;

fn returned(success: bool, value: i32) -> FunctionOutcome {
    FunctionOutcome::Returned { success, value }
}

fn compile(functions: &[(&str, &str)], tags: &[(&str, &str)]) -> Vm {
    load_functions_and_tags(functions.iter().copied(), tags.iter().copied()).unwrap()
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
    Vm::from_packs([Pack::memory(functions.chain(tags))])
}

#[test]
fn nested_tags_expand_in_first_occurrence_order() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #trace state 0\nscoreboard players set #ten state 10\n",
            ),
            (
                "example:first",
                "scoreboard players set #trace state 1\nreturn 1\n",
            ),
            (
                "example:second",
                "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 2\nreturn 2\n",
            ),
            (
                "example:third",
                "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 3\nreturn 3\n",
            ),
            (
                "example:main",
                "function #example:outer\nreturn run scoreboard players get #trace state\n",
            ),
        ],
        &[
            (
                "example:outer",
                r##"{"values":["example:first","#example:nested","example:second",{"id":"example:missing","required":false},{"id":"#example:missing","required":false},"example:third"]}"##,
            ),
            (
                "example:nested",
                r#"{"replace":true,"values":["example:second","example:first"]}"#,
            ),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:main", LIMIT).unwrap(),
        returned(true, 123)
    );
}

#[test]
fn optional_cycles_omit_only_the_edge_that_closes_the_cycle() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #trace state 0\nscoreboard players set #ten state 10\n",
            ),
            (
                "example:a",
                "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 1\n",
            ),
            (
                "example:b",
                "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 2\n",
            ),
            (
                "example:main",
                "function #example:b_tag\nreturn run scoreboard players get #trace state\n",
            ),
        ],
        &[
            (
                "example:a_tag",
                r##"{"values":[{"id":"#example:b_tag","required":false},"example:a"]}"##,
            ),
            (
                "example:b_tag",
                r##"{"values":["#example:a_tag","example:b"]}"##,
            ),
            (
                "example:self",
                r##"{"values":[{"id":"#example:self","required":false}]}"##,
            ),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:main", LIMIT).unwrap(),
        returned(true, 12)
    );
}

#[test]
fn deeply_nested_tags_are_resolved_without_rust_recursion() {
    const DEPTH: usize = 4096;

    let tags = (0..DEPTH)
        .map(|index| {
            let id = format!("example:tag_{index:04}");
            let source = if index + 1 == DEPTH {
                r#"{"values":[]}"#.to_owned()
            } else {
                format!(r##"{{"values":["#example:tag_{:04}"]}}"##, index + 1)
            };
            (id, source)
        })
        .collect::<Vec<_>>();

    load_functions_and_tags(
        std::iter::empty::<(&str, &str)>(),
        tags.iter()
            .map(|(id, source)| (id.as_str(), source.as_str())),
    )
    .unwrap();
}

#[test]
fn callbackless_members_use_the_tag_calls_discard_scope() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #trace state 0\n",
            ),
            ("example:reset", "scoreboard players set #trace state 0\n"),
            (
                "example:no_callback",
                "return run function example:missing\n",
            ),
            (
                "example:later",
                "scoreboard players add #trace state 1\nreturn 7\n",
            ),
            (
                "example:normal",
                "function #example:callbacks\nreturn run scoreboard players get #trace state\n",
            ),
            (
                "example:returning",
                "return run function #example:callbacks\n",
            ),
            (
                "example:if_condition",
                "execute if function #example:callbacks run scoreboard players add #trace state 100\nreturn run scoreboard players get #trace state\n",
            ),
            (
                "example:unless_condition",
                "execute unless function #example:callbacks run scoreboard players add #trace state 100\nreturn run scoreboard players get #trace state\n",
            ),
            (
                "example:read_trace",
                "return run scoreboard players get #trace state\n",
            ),
        ],
        &[(
            "example:callbacks",
            r#"{"values":["example:no_callback","example:later"]}"#,
        )],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:normal", LIMIT).unwrap(),
        returned(true, 1)
    );

    vm.execute_function("example:reset", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:returning", LIMIT).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:read_trace", LIMIT).unwrap(),
        returned(true, 0)
    );

    for function in ["example:if_condition", "example:unless_condition"] {
        vm.execute_function("example:reset", LIMIT).unwrap();
        assert_eq!(
            vm.execute_function(function, LIMIT).unwrap(),
            returned(true, 0)
        );
    }
}

#[test]
fn normal_tag_calls_aggregate_only_multi_member_callbacks() {
    let mut vm = compile(
        &[
            ("example:setup", "scoreboard objectives add state dummy\n"),
            ("example:fail", "return fail\n"),
            ("example:fallthrough_a", "# no return\n"),
            ("example:fallthrough_b", "# still no return\n"),
            ("example:max", "return 2147483647\n"),
            ("example:one", "return 1\n"),
            (
                "example:single_failure",
                "scoreboard players set #stored state 9\nexecute store success score #stored state run function #example:single_failure\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:multi_failure",
                "scoreboard players set #stored state 9\nexecute store success score #stored state run function #example:multi_failure\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:wrapping_sum",
                "execute store result score #stored state run function #example:wrapping_sum\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:all_fallthrough",
                "scoreboard players set #stored state 9\nexecute store result score #stored state run function #example:all_fallthrough\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:empty",
                "scoreboard players set #stored state 9\nexecute store result score #stored state run function #example:empty\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:unknown",
                "scoreboard players set #stored state 9\nexecute store result score #stored state run function #example:unknown\nreturn run scoreboard players get #stored state\n",
            ),
        ],
        &[
            ("example:single_failure", r#"{"values":["example:fail"]}"#),
            (
                "example:multi_failure",
                r#"{"values":["example:fail","example:fallthrough_a"]}"#,
            ),
            (
                "example:wrapping_sum",
                r#"{"values":["example:max","example:one"]}"#,
            ),
            (
                "example:all_fallthrough",
                r#"{"values":["example:fallthrough_a","example:fallthrough_b"]}"#,
            ),
            ("example:empty", r#"{"values":[]}"#),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:single_failure", LIMIT)
            .unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:multi_failure", LIMIT).unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:wrapping_sum", LIMIT).unwrap(),
        returned(true, i32::MIN)
    );
    assert_eq!(
        vm.execute_function("example:all_fallthrough", LIMIT)
            .unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:empty", LIMIT).unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:unknown", LIMIT).unwrap(),
        returned(true, 0)
    );
}

#[test]
fn return_run_tags_stop_at_the_first_member_result() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #late state 0\nscoreboard players set #stored state 9\n",
            ),
            ("example:fallthrough_a", "# no return\n"),
            ("example:fallthrough_b", "# no return either\n"),
            ("example:seven", "return 7\n"),
            (
                "example:late",
                "scoreboard players add #late state 1\nreturn 9\n",
            ),
            (
                "example:first_result",
                "return run function #example:first_result\n",
            ),
            (
                "example:first_result_stored",
                "return run execute store result score #stored state run function #example:first_result\n",
            ),
            (
                "example:all_fallthrough",
                "return run function #example:all_fallthrough\n",
            ),
            (
                "example:all_fallthrough_stored",
                "return run execute store result score #stored state run function #example:all_fallthrough\n",
            ),
            (
                "example:one_fallthrough",
                "return run function #example:one_fallthrough\n",
            ),
            (
                "example:empty",
                "return run execute store success score #stored state run function #example:empty\n",
            ),
            (
                "example:unknown",
                "return run execute store success score #stored state run function #example:unknown\n",
            ),
            (
                "example:read_late",
                "return run scoreboard players get #late state\n",
            ),
            (
                "example:read_stored",
                "return run scoreboard players get #stored state\n",
            ),
            (
                "example:reset_stored",
                "scoreboard players set #stored state 9\n",
            ),
        ],
        &[
            (
                "example:first_result",
                r#"{"values":["example:fallthrough_a","example:seven","example:late"]}"#,
            ),
            (
                "example:all_fallthrough",
                r#"{"values":["example:fallthrough_a","example:fallthrough_b"]}"#,
            ),
            (
                "example:one_fallthrough",
                r#"{"values":["example:fallthrough_a"]}"#,
            ),
            ("example:empty", r#"{"values":[]}"#),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:first_result", LIMIT).unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read_late", LIMIT).unwrap(),
        returned(true, 0)
    );
    assert_eq!(
        vm.execute_function("example:first_result_stored", LIMIT)
            .unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 7)
    );
    vm.execute_function("example:reset_stored", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:all_fallthrough_stored", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:all_fallthrough", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:one_fallthrough", LIMIT)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:empty", LIMIT).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 0)
    );
    vm.execute_function("example:reset_stored", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:unknown", LIMIT).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 0)
    );
}

#[test]
fn function_tag_conditions_share_one_isolated_short_circuit() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #calls state 0\nscoreboard players set #stored state 9\n",
            ),
            (
                "example:fallthrough",
                "scoreboard players add #calls state 10\n",
            ),
            (
                "example:fallthrough_b",
                "scoreboard players add #calls state 20\n",
            ),
            (
                "example:zero",
                "scoreboard players add #calls state 1\nreturn 0\n",
            ),
            (
                "example:positive",
                "scoreboard players add #calls state 100\nreturn 2\n",
            ),
            (
                "example:if_zero",
                "return run execute if function #example:zero_result run return 4\n",
            ),
            (
                "example:unless_zero",
                "return run execute unless function #example:zero_result run return 5\n",
            ),
            (
                "example:passing_store",
                "execute store result score #stored state if function #example:positive_result run return 7\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:all_fallthrough",
                "return run execute unless function #example:all_fallthrough run return 12\n",
            ),
            (
                "example:empty_if",
                "execute if function #example:empty run return 8\nreturn 6\n",
            ),
            (
                "example:empty_unless",
                "execute unless function #example:empty run return 8\nreturn 6\n",
            ),
            (
                "example:empty_return",
                "return run execute unless function #example:empty run return 8\nreturn 6\n",
            ),
            (
                "example:unknown_return",
                "return run execute if function #example:unknown run return 8\n",
            ),
            (
                "example:read_calls",
                "return run scoreboard players get #calls state\n",
            ),
            (
                "example:read_stored",
                "return run scoreboard players get #stored state\n",
            ),
        ],
        &[
            (
                "example:zero_result",
                r#"{"values":["example:fallthrough","example:zero","example:positive"]}"#,
            ),
            (
                "example:positive_result",
                r#"{"values":["example:fallthrough","example:positive"]}"#,
            ),
            (
                "example:all_fallthrough",
                r#"{"values":["example:fallthrough","example:fallthrough_b"]}"#,
            ),
            ("example:empty", r#"{"values":[]}"#),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:if_zero", LIMIT).unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:unless_zero", LIMIT).unwrap(),
        returned(true, 5)
    );
    assert_eq!(
        vm.execute_function("example:read_calls", LIMIT).unwrap(),
        returned(true, 22)
    );
    assert_eq!(
        vm.execute_function("example:passing_store", LIMIT).unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 7)
    );
    assert_eq!(
        vm.execute_function("example:all_fallthrough", LIMIT)
            .unwrap(),
        returned(true, 12)
    );
    assert_eq!(
        vm.execute_function("example:empty_if", LIMIT).unwrap(),
        returned(true, 6)
    );
    assert_eq!(
        vm.execute_function("example:empty_unless", LIMIT).unwrap(),
        returned(true, 6)
    );
    assert_eq!(
        vm.execute_function("example:empty_return", LIMIT).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:unknown_return", LIMIT)
            .unwrap(),
        FunctionOutcome::FellThrough
    );
}

#[test]
fn tag_member_calls_own_the_only_tag_execution_cost() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #stored state 9\n",
            ),
            ("example:first", "return 1\n"),
            ("example:second", "return 2\n"),
            ("example:fallthrough_a", "# no return\n"),
            ("example:fallthrough_b", "# no return either\n"),
            ("example:normal", "function #example:both\nreturn 9\n"),
            ("example:returning", "return run function #example:both\n"),
            (
                "example:condition",
                "return run execute if function #example:both run return 9\n",
            ),
            ("example:recursive", "function #example:recursive\n"),
            (
                "example:aggregate_limit",
                "execute store result score #stored state run function #example:both\n",
            ),
            (
                "example:returning_fallthrough",
                "return run function #example:all_fallthrough\n",
            ),
            (
                "example:condition_fallthrough",
                "return run execute unless function #example:all_fallthrough run return 9\n",
            ),
            (
                "example:read_stored",
                "return run scoreboard players get #stored state\n",
            ),
        ],
        &[
            (
                "example:both",
                r#"{"values":["example:first","example:second"]}"#,
            ),
            (
                "example:all_fallthrough",
                r#"{"values":["example:fallthrough_a","example:fallthrough_b"]}"#,
            ),
            ("example:recursive", r#"{"values":["example:recursive"]}"#),
        ],
    );

    vm.execute_function("example:setup", LIMIT).unwrap();
    assert_eq!(
        vm.execute_function("example:normal", 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:normal", 4).unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:returning", 3).unwrap(),
        returned(true, 1)
    );
    assert_eq!(
        vm.execute_function("example:condition", 3).unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:recursive", 10),
        Err(ExecutionError::CommandLimitExceeded { limit: 10 })
    );
    assert_eq!(
        vm.execute_function("example:aggregate_limit", 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:aggregate_limit", 4),
        Err(ExecutionError::CommandLimitExceeded { limit: 4 })
    );
    assert_eq!(
        vm.execute_function("example:aggregate_limit", 5).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:read_stored", LIMIT).unwrap(),
        returned(true, 3)
    );
    assert_eq!(
        vm.execute_function("example:returning_fallthrough", 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:returning_fallthrough", 4)
            .unwrap(),
        returned(false, 0)
    );
    assert_eq!(
        vm.execute_function("example:condition_fallthrough", 3),
        Err(ExecutionError::CommandLimitExceeded { limit: 3 })
    );
    assert_eq!(
        vm.execute_function("example:condition_fallthrough", 4)
            .unwrap(),
        returned(true, 9)
    );
}

#[test]
fn invalid_function_tag_resources_are_rejected_atomically() {
    assert!(matches!(
        load_functions_and_tags(
            [("example:function", "return 1\n")],
            [("Upper:tag", r#"{"values":[]}"#)],
        )
        .unwrap_err(),
        LoadError::InvalidMemoryResourceIdentifier {
            pack: 0,
            kind: ResourceKind::FunctionTag,
            input,
        } if input == "Upper:tag"
    ));
    assert!(matches!(
        load_functions_and_tags(
            [("example:function", "return 1\n")],
            [("tag", r#"{"values":[]}"#), (":tag", r#"{"values":[]}"#),],
        )
        .unwrap_err(),
        LoadError::DuplicateMemoryResource {
            pack: 0,
            kind: ResourceKind::FunctionTag,
            id,
        } if id == "minecraft:tag"
    ));

    for (tag, source, expected_reason) in [
        ("example:invalid_json", "not json", "invalid JSON"),
        (
            "example:missing_function",
            r#"{"values":["example:missing"]}"#,
            "required function `example:missing` does not exist",
        ),
        (
            "example:missing_tag",
            r##"{"values":["#example:missing"]}"##,
            "required function tag `#example:missing` does not exist",
        ),
        (
            "example:invalid_replace",
            r#"{"replace":0,"values":[]}"#,
            "`replace` must be a boolean",
        ),
        (
            "example:invalid_entry",
            r#"{"values":[{"id":"example:function","required":null}]}"#,
            "`values[0].required` must be a boolean",
        ),
    ] {
        match load_functions_and_tags([("example:function", "return 1\n")], [(tag, source)])
            .unwrap_err()
        {
            LoadError::InvalidFunctionTag { origin, reason } => {
                assert_eq!(
                    origin,
                    ResourceOrigin::Memory {
                        pack: 0,
                        id: tag.to_owned()
                    }
                );
                assert!(reason.contains(expected_reason), "{reason:?}");
            }
            error => panic!("expected an invalid function tag, got {error}"),
        }
    }

    match load_functions_and_tags(
        [("example:function", "return 1\n")],
        [
            ("example:a", r##"{"values":["#example:b"]}"##),
            ("example:b", r##"{"values":["#example:a"]}"##),
        ],
    )
    .unwrap_err()
    {
        LoadError::InvalidFunctionTag { reason, .. } => {
            assert!(reason.contains("cyclic"), "{reason:?}");
        }
        error => panic!("expected a cyclic function tag, got {error}"),
    }
}
