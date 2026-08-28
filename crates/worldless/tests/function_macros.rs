mod common;

use common::context;
use worldless::{ExecutionOutcome, LoadError, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 512;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
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
        0,
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
    Vm::from_packs([Pack::memory(functions.chain(tags))], 0)
}

fn assert_function(vm: &mut Vm, function: &str, expected: ExecutionOutcome) {
    assert_eq!(
        vm.execute_function(function, None, context(), LIMIT, drop)
            .unwrap(),
        expected,
        "{function}"
    );
}

#[test]
fn direct_and_storage_arguments_instantiate_macro_functions() {
    let mut vm = compile(&[
        ("example:value", "$return $(value)\n"),
        (
            "example:direct",
            "return run function example:value {value:7b,unused:99}\n",
        ),
        (
            "example:string",
            "return run function example:value {value:\"8\"}\n",
        ),
        (
            "example:setup",
            "data merge storage example:args {value:9,nested:{value:10}}\n",
        ),
        (
            "example:root",
            "return run function example:value with storage example:args\n",
        ),
        (
            "example:path",
            "return run function example:value with storage example:args nested\n",
        ),
        ("example:plain", "return 11\n"),
        (
            "example:plain_missing_storage",
            "return run function example:plain with storage example:missing\n",
        ),
    ]);

    assert_function(&mut vm, "example:direct", returned(true, 7));
    assert_function(&mut vm, "example:string", returned(true, 8));
    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:root", returned(true, 9));
    assert_function(&mut vm, "example:path", returned(true, 10));
    assert_function(&mut vm, "example:plain_missing_storage", returned(true, 11));
    assert_function(&mut vm, "example:direct", returned(true, 7));
}

#[test]
fn template_validation_matches_java_utf16_rules() {
    for (source, line) in [
        ("$return 1\n", 1),
        ("$return $(value\n", 1),
        ("$return $(bad-name)\n", 1),
        ("$return $(\u{10400})\n", 1),
    ] {
        assert!(matches!(
            load_functions([("example:invalid", source)]),
            Err(LoadError::InvalidFunction {
                line: actual_line,
                ..
            }) if actual_line == line
        ));
    }

    let mut vm = compile(&[
        ("example:bmp", "$return $(é)\n"),
        (
            "example:call_bmp",
            "return run function example:bmp {\"é\":3}\n",
        ),
    ]);
    assert_function(&mut vm, "example:call_bmp", returned(true, 3));
}

#[test]
fn substitution_preserves_compact_snbt_and_java_utf16() {
    let mut vm = compile(&[
        (
            "example:copy",
            "$data modify storage example:out copied set value $(value)\n",
        ),
        (
            "example:invoke",
            r#"function example:copy {value:{z:[L;-1L,2L],quoted:"a\"b",slash:"a\\b",control:"a\nb",surrogate:"\uD800"}}
"#,
        ),
        (
            "example:verify",
            r#"execute if data storage example:out copied{z:[L;-1L,2L],quoted:"a\"b",slash:"a\\b",control:"a\nb",surrogate:"\uD800"} run return 1
return fail
"#,
        ),
        (
            "example:raw_string",
            "$data modify storage example:out raw set value $(value)\n",
        ),
        (
            "example:invoke_raw",
            "function example:raw_string {value:\"17\"}\nreturn run data get storage example:out raw\n",
        ),
    ]);

    assert_function(&mut vm, "example:invoke", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:verify", returned(true, 1));
    assert_function(&mut vm, "example:invoke_raw", returned(true, 17));
}

#[test]
fn dynamic_score_holders_preserve_distinct_java_utf16_values() {
    let mut vm = compile(&[
        (
            "example:set_score",
            "$return run scoreboard players set $(holder) values $(value)\n",
        ),
        (
            "example:get_score",
            "$return run scoreboard players get $(holder) values\n",
        ),
        ("example:setup", "scoreboard objectives add values dummy\n"),
        (
            "example:set_first",
            r##"return run function example:set_score {holder:"#\uD800",value:7}
"##,
        ),
        (
            "example:set_second",
            r##"return run function example:set_score {holder:"#\uD801",value:9}
"##,
        ),
        (
            "example:first",
            r##"return run function example:get_score {holder:"#\uD800"}
"##,
        ),
        (
            "example:second",
            r##"return run function example:get_score {holder:"#\uD801"}
"##,
        ),
    ]);

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:set_first", returned(true, 7));
    assert_function(&mut vm, "example:set_second", returned(true, 9));
    assert_function(&mut vm, "example:first", returned(true, 7));
    assert_function(&mut vm, "example:second", returned(true, 9));
}

#[test]
fn numeric_arguments_follow_macro_stringification_rules() {
    let mut vm = compile(&[
        (
            "example:wrap",
            "$data modify storage example:out result set value {x:$(value)}\n",
        ),
        ("example:small_long", "function example:wrap {value:4L}\n"),
        ("example:float", "function example:wrap {value:0.1f}\n"),
        ("example:double", "function example:wrap {value:0.1d}\n"),
        (
            "example:nested",
            "function example:wrap {value:{long:4L,float:1.0f}}\n",
        ),
        (
            "example:large_long",
            "execute store success storage example:out status int 1 run function example:wrap {value:9223372036854775807L}\n",
        ),
        (
            "example:check_small_long",
            "execute if data storage example:out result{x:4} run return 1\nreturn fail\n",
        ),
        (
            "example:check_float",
            "execute if data storage example:out result{x:.100000001490116d} run return 1\nreturn fail\n",
        ),
        (
            "example:check_double",
            "execute if data storage example:out result{x:.1d} run return 1\nreturn fail\n",
        ),
        (
            "example:check_nested",
            "execute if data storage example:out result{x:{long:4L,float:1.0f}} run return 1\nreturn fail\n",
        ),
        (
            "example:check_failure",
            "return run data get storage example:out status\n",
        ),
    ]);

    assert_function(&mut vm, "example:small_long", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:check_small_long", returned(true, 1));
    assert_function(&mut vm, "example:float", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:check_float", returned(true, 1));
    assert_function(&mut vm, "example:double", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:check_double", returned(true, 1));
    assert_function(&mut vm, "example:nested", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:check_nested", returned(true, 1));
    assert_function(&mut vm, "example:large_long", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:check_failure", returned(true, 0));
}

#[test]
fn instantiation_is_atomic_and_failures_are_command_results() {
    let mut vm = compile(&[
        (
            "example:macro",
            "scoreboard players add #side values 1\n$return $(result)\n",
        ),
        (
            "example:setup",
            "scoreboard objectives add values dummy\nscoreboard players set #side values 0\nscoreboard players set #status values 9\ndata merge storage example:args {scalar:1,list:[{result:2},{result:3}]}\n",
        ),
        (
            "example:bad_parse",
            "execute store success score #status values run function example:macro {result:\"invalid\"}\nreturn run scoreboard players get #side values\n",
        ),
        (
            "example:missing",
            "execute store success score #status values run function example:macro {}\nreturn run scoreboard players get #side values\n",
        ),
        (
            "example:status",
            "return run scoreboard players get #status values\n",
        ),
        (
            "example:bad_path",
            "execute store success score #status values run function example:macro with storage example:args missing\n",
        ),
        (
            "example:scalar_path",
            "execute store success score #status values run function example:macro with storage example:args scalar\n",
        ),
        (
            "example:multiple_path",
            "execute store success score #status values run function example:macro with storage example:args list[]\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:bad_parse", returned(true, 0));
    assert_function(&mut vm, "example:status", returned(true, 0));
    assert_function(&mut vm, "example:missing", returned(true, 0));
    assert_function(&mut vm, "example:status", returned(true, 0));
    assert_function(&mut vm, "example:bad_path", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:status", returned(true, 0));
    assert_function(&mut vm, "example:scalar_path", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:status", returned(true, 0));
    assert_function(&mut vm, "example:multiple_path", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:status", returned(true, 0));
}

#[test]
fn nested_macros_require_explicit_arguments_and_tags_share_one_snapshot() {
    let mut vm = load_functions_and_tags(
        [
            ("example:inner", "$return $(value)\n"),
            (
                "example:outer",
                "$return run function example:inner {value:$(value)}\n",
            ),
            (
                "example:no_inheritance",
                "$function example:inner$(suffix)\nreturn 5\n",
            ),
            ("example:first", "$return $(value)\n"),
            ("example:second", "$return $(value)\n"),
            (
                "example:setup",
                "scoreboard objectives add values dummy\nscoreboard players set #sum values 0\n",
            ),
            (
                "example:call_outer",
                "return run function example:outer {value:12}\n",
            ),
            (
                "example:call_without_inheritance",
                "return run function example:no_inheritance {suffix:\"\"}\n",
            ),
            (
                "example:call_tag",
                "execute store result score #sum values run function #example:both {value:6}\nreturn run scoreboard players get #sum values\n",
            ),
        ],
        [("example:both", r#"{"values":["example:first","example:second"]}"#)],
    )
    .unwrap();

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:call_outer", returned(true, 12));
    assert_function(
        &mut vm,
        "example:call_without_inheritance",
        returned(true, 5),
    );
    assert_function(&mut vm, "example:call_tag", returned(true, 12));
}

#[test]
fn tag_instantiation_keeps_a_successful_prefix_and_snapshots_storage() {
    let mut vm = load_functions_and_tags(
        [
            (
                "example:plain_prefix",
                "scoreboard players add #prefix values 1\nreturn 4\n",
            ),
            ("example:bad", "$return $(missing)\n"),
            (
                "example:late",
                "scoreboard players add #late values 1\nreturn 8\n",
            ),
            (
                "example:mutate",
                "data modify storage example:args value set value 9\n$return $(value)\n",
            ),
            ("example:read_snapshot", "$return $(value)\n"),
            (
                "example:setup",
                "scoreboard objectives add values dummy\nscoreboard players set #prefix values 0\nscoreboard players set #late values 0\nscoreboard players set #stored values 7\ndata merge storage example:args {value:1}\n",
            ),
            (
                "example:failed_tag",
                "execute store result score #stored values run function #example:failing {other:1}\nreturn run scoreboard players get #stored values\n",
            ),
            (
                "example:failed_tag_as_return",
                "return run function #example:failing {other:1}\n",
            ),
            (
                "example:condition_prefix",
                "execute if function #example:failing run return 1\nreturn fail\n",
            ),
            (
                "example:read_prefix",
                "return run scoreboard players get #prefix values\n",
            ),
            (
                "example:read_late",
                "return run scoreboard players get #late values\n",
            ),
            (
                "example:snapshot",
                "execute store result score #stored values run function #example:snapshot with storage example:args\nreturn run scoreboard players get #stored values\n",
            ),
        ],
        [
            (
                "example:failing",
                r#"{"values":["example:plain_prefix","example:bad","example:late"]}"#,
            ),
            (
                "example:snapshot",
                r#"{"values":["example:mutate","example:read_snapshot"]}"#,
            ),
        ],
    )
    .unwrap();

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:failed_tag", returned(true, 0));
    assert_function(&mut vm, "example:read_prefix", returned(true, 1));
    assert_function(&mut vm, "example:read_late", returned(true, 0));
    assert_function(&mut vm, "example:failed_tag_as_return", returned(true, 4));
    assert_function(&mut vm, "example:condition_prefix", returned(true, 1));
    assert_function(&mut vm, "example:read_late", returned(true, 0));
    assert_function(&mut vm, "example:snapshot", returned(true, 2));
}

#[test]
fn macro_conditions_have_no_arguments_and_top_level_failure_is_explicit() {
    let mut vm = compile(&[
        ("example:macro", "$return $(value)\n"),
        (
            "example:unless_macro",
            "execute unless function example:macro run return 1\nreturn fail\n",
        ),
    ]);

    assert_function(&mut vm, "example:unless_macro", returned(true, 1));
    assert_eq!(
        vm.execute_function("example:macro", None, context(), LIMIT, drop)
            .unwrap(),
        returned(false, 0)
    );

    for command in [
        "function example:macro with entity @s",
        "function example:macro with block 0 0 0",
        "execute if function example:macro with storage example:args run return 1",
    ] {
        assert!(matches!(
            load_functions([("example:invalid", command)]),
            Err(LoadError::InvalidFunction { .. })
        ));
    }
}

#[test]
fn macro_instantiation_does_not_consume_command_quota() {
    let mut vm = compile(&[
        ("example:plain_target", "return 7\n"),
        ("example:macro_target", "$return $(value)\n"),
        (
            "example:plain_call",
            "return run function example:plain_target\n",
        ),
        (
            "example:macro_call",
            "return run function example:macro_target {value:7}\n",
        ),
    ]);

    for limit in 1..=8 {
        assert_eq!(
            vm.execute_function("example:plain_call", None, context(), limit, drop),
            vm.execute_function("example:macro_call", None, context(), limit, drop),
            "limit {limit}"
        );
    }
}
