mod common;

use common::context;
use worldless::{ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 16_384;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn function(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::Function, id, source)
}

fn execute(vm: &mut Vm, command: &str) -> ExecutionOutcome {
    vm.execute_command(command, context(), LIMIT, drop).unwrap()
}

fn concat_vm() -> Vm {
    let functions = [
        function(
            "concat:concat",
            r#"scoreboard objectives add concat dummy

data remove storage concat: result

execute store result score #expected concat run data get storage concat: first
execute store result score #actual concat run data get storage concat: second
scoreboard players operation #expected concat += #actual concat

function concat:concat/single_quotes.macro with storage concat:
execute store result score #actual concat run data get storage concat: result
execute if score #expected concat = #actual concat if data storage concat: result run return 1

function concat:concat/double_quotes.macro with storage concat:
execute store result score #actual concat run data get storage concat: result
execute if score #expected concat = #actual concat if data storage concat: result run return 2

data modify storage concat: parts set value [[], []]
data modify storage concat: decompose set from storage concat: first
function concat:concat/decompose with storage concat:

data modify storage concat: parts append value []
data modify storage concat: decompose set from storage concat: second
function concat:concat/decompose with storage concat:

function concat:concat/compose
data modify storage concat: result set from storage concat: tokens[0]
data remove storage concat: tokens
"#,
        ),
        function(
            "concat:concat/single_quotes.macro",
            r#"$data modify storage concat: result set value '$(first)$(second)'
"#,
        ),
        function(
            "concat:concat/double_quotes.macro",
            r#"$data modify storage concat: result set value "$(first)$(second)"
"#,
        ),
        function(
            "concat:concat/decompose",
            r#"scoreboard players set #marker concat 0
scoreboard players set #index concat 0
execute store result score #length concat run data get storage concat: decompose
function concat:concat/decompose/iterate

execute store result storage concat: start int 1 run scoreboard players get #marker concat
execute store result storage concat: end int 1 run scoreboard players get #length concat
execute if score #marker concat < #length concat run function concat:concat/decompose/append.macro with storage concat:

data remove storage concat: start
data remove storage concat: end
data remove storage concat: char
data remove storage concat: decompose
"#,
        ),
        function(
            "concat:concat/decompose/iterate",
            r#"execute store result storage concat: start int 1 run scoreboard players get #index concat
execute store result storage concat: end int 1 run scoreboard players add #index concat 1
function concat:concat/decompose/char_at.macro with storage concat:

execute if data storage concat: {char: '"'} run function concat:concat/decompose/split
execute if data storage concat: {char: '\\'} run function concat:concat/decompose/split

execute if score #index concat < #length concat run function concat:concat/decompose/iterate
"#,
        ),
        function(
            "concat:concat/decompose/char_at.macro",
            r#"$data modify storage concat: char set string storage concat: decompose $(start) $(end)
"#,
        ),
        function(
            "concat:concat/decompose/split",
            r#"execute store result storage concat: start int 1 run scoreboard players get #marker concat
execute store result storage concat: end int 0.9999999999999999 run scoreboard players get #index concat
execute if score #marker concat < #index concat run function concat:concat/decompose/append.macro with storage concat:

data modify storage concat: parts[-2] append from storage concat: char
scoreboard players operation #marker concat = #index concat
"#,
        ),
        function(
            "concat:concat/decompose/append.macro",
            r#"$data modify storage concat: parts[-2] append string storage concat: decompose $(start) $(end)
"#,
        ),
        function(
            "concat:concat/compose",
            r#"data modify storage concat: left set from storage concat: parts[0][-1]
data modify storage concat: right set from storage concat: parts[1][0]
execute unless data storage concat: {left: '"'} \
        unless data storage concat: {left: '\\'} \
        unless data storage concat: {right: '"'} \
        unless data storage concat: {right: '\\'} \
        run function concat:concat/compose/joint.macro with storage concat:

data modify storage concat: tokens append from storage concat: parts[][]
data remove storage concat: parts

data modify storage concat: escape set value '\\'
execute store result score #length concat run data get storage concat: tokens
function concat:concat/compose/double_escape.macro with storage concat:
data modify storage concat: escape set string storage concat: escape 1

data modify storage concat: left set from storage concat: tokens[-2]
data modify storage concat: right set from storage concat: tokens[-1]
execute if data storage concat: {right: '"'} run function concat:concat/compose/escape_right.macro with storage concat:
execute if data storage concat: {right: '\\'} run function concat:concat/compose/escape_right.macro with storage concat:

execute if data storage concat: tokens[1] run function concat:concat/compose/iterate_left

data remove storage concat: left
data remove storage concat: right
data remove storage concat: escape
"#,
        ),
        function(
            "concat:concat/compose/joint.macro",
            r#"$data modify storage concat: parts[1][0] set value "$(left)$(right)"
data remove storage concat: parts[0][-1]
"#,
        ),
        function(
            "concat:concat/compose/double_escape.macro",
            r#"$data modify storage concat: escape set value "$(escape)$(escape)$(escape)$(escape)"
scoreboard players remove #length concat 1
execute if score #length concat matches 2.. run function concat:concat/compose/double_escape.macro with storage concat:
"#,
        ),
        function(
            "concat:concat/compose/halve_escape.macro",
            r#"$data modify storage concat: escape set value "\$(escape)"
data modify storage concat: escape set string storage concat: escape 1
"#,
        ),
        function(
            "concat:concat/compose/escape_right.macro",
            r#"$data modify storage concat: tokens[-2] set value "$(left)$(escape)$(right)"
data remove storage concat: tokens[-1]

function concat:concat/compose/halve_escape.macro with storage concat:
"#,
        ),
        function(
            "concat:concat/compose/iterate_left",
            r#"data modify storage concat: left set from storage concat: tokens[-2]
data modify storage concat: right set from storage concat: tokens[-1]

function concat:concat/compose/escape_left.macro with storage concat:

execute if data storage concat: tokens[1] run function concat:concat/compose/iterate_neither
"#,
        ),
        function(
            "concat:concat/compose/escape_left.macro",
            r#"$data modify storage concat: tokens[-2] set value "$(escape)$(left)$(right)"
data remove storage concat: tokens[-1]

function concat:concat/compose/halve_escape.macro with storage concat:
"#,
        ),
        function(
            "concat:concat/compose/iterate_neither",
            r#"data modify storage concat: left set from storage concat: tokens[-2]
data modify storage concat: right set from storage concat: tokens[-1]

function concat:concat/compose/escape_neither.macro with storage concat:

execute if data storage concat: tokens[1] run function concat:concat/compose/iterate_left
"#,
        ),
        function(
            "concat:concat/compose/escape_neither.macro",
            r#"$data modify storage concat: tokens[-2] set value "$(left)$(right)"
data remove storage concat: tokens[-1]

function concat:concat/compose/halve_escape.macro with storage concat:
"#,
        ),
    ];
    Vm::from_packs([Pack::memory(functions)], 0).unwrap()
}

#[test]
fn adapted_concat_preserves_strings_across_fast_fallback_and_slow_paths() {
    struct Case {
        name: &'static str,
        first: &'static str,
        second: &'static str,
        expected: &'static str,
        returned: Option<i32>,
    }

    let cases = [
        Case {
            name: "ordinary fast path",
            first: r#""foo""#,
            second: r#""bar""#,
            expected: r#""foobar""#,
            returned: Some(1),
        },
        Case {
            name: "single quote falls back to double quotes",
            first: r#""a'b""#,
            second: r#""c""#,
            expected: r#""a'bc""#,
            returned: Some(2),
        },
        Case {
            name: "escape-looking boundary uses the slow path",
            first: r#""\\""#,
            second: r#""n""#,
            expected: r#""\\n""#,
            returned: None,
        },
        Case {
            name: "empty strings",
            first: r#""""#,
            second: r#""""#,
            expected: r#""""#,
            returned: Some(1),
        },
        Case {
            name: "README example",
            first: r#""'hello' \\ ""#,
            second: r#"'"world"'"#,
            expected: r#""'hello' \\ \"world\"""#,
            returned: None,
        },
    ];

    for case in cases {
        let mut vm = concat_vm();
        assert_eq!(
            execute(
                &mut vm,
                &format!("data modify storage concat: first set value {}", case.first),
            ),
            returned(true, 1),
            "{} first input",
            case.name
        );
        assert_eq!(
            execute(
                &mut vm,
                &format!(
                    "data modify storage concat: second set value {}",
                    case.second
                ),
            ),
            returned(true, 1),
            "{} second input",
            case.name
        );

        let expected_outcome = case
            .returned
            .map_or(ExecutionOutcome::NoResult, |value| returned(true, value));
        assert_eq!(
            vm.execute_function("concat:concat", None, context(), LIMIT, drop)
                .unwrap(),
            expected_outcome,
            "{} function result",
            case.name
        );
        assert_eq!(
            execute(
                &mut vm,
                &format!(
                    "execute if data storage concat: {{result:{}}}",
                    case.expected
                ),
            ),
            returned(true, 1),
            "{} concatenated value",
            case.name
        );
    }
}

#[test]
fn deep_wildcard_filter_removal_counts_matches_and_preserves_siblings() {
    let mut vm = Vm::from_packs([Pack::memory(std::iter::empty::<MemoryResource>())], 0).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            "data merge storage heap:gc {_: [{_: [{_: [{_:{_count:1610612640},value:1},{_:{_count:1},value:2}]},{_: [{_:{_count:1610612640},value:3}]}]},{_: [{_: [{_:{_count:1610612640},value:4},{_:{_count:2},value:5}]}]}]}",
        ),
        returned(true, 1)
    );

    let dead = "_[]._[]._[{_:{_count:1610612640}}]";
    assert_eq!(
        execute(&mut vm, &format!("execute if data storage heap:gc {dead}")),
        returned(true, 3)
    );
    assert_eq!(
        execute(&mut vm, &format!("data remove storage heap:gc {dead}")),
        returned(true, 3)
    );
    assert_eq!(
        execute(&mut vm, &format!("execute if data storage heap:gc {dead}")),
        returned(false, 0)
    );
    assert_eq!(
        execute(&mut vm, "execute if data storage heap:gc _[]._[]._[]",),
        returned(true, 2)
    );
    for value in [2, 5] {
        assert_eq!(
            execute(
                &mut vm,
                &format!("execute if data storage heap:gc _[]._[]._[{{value:{value}}}]")
            ),
            returned(true, 1),
            "surviving value {value}"
        );
    }
}

#[test]
fn mixed_score_and_storage_stores_receive_the_same_command_result() {
    let mut vm = Vm::from_packs([Pack::memory(std::iter::empty::<MemoryResource>())], 0).unwrap();
    assert_eq!(
        execute(&mut vm, "scoreboard objectives add heap dummy"),
        returned(true, 1)
    );
    assert_eq!(
        execute(&mut vm, "data merge storage heap:source {value:37}",),
        returned(true, 1)
    );

    assert_eq!(
        execute(
            &mut vm,
            "execute store result score #copy heap store result storage heap:stored value int 1 run data get storage heap:source value",
        ),
        returned(true, 37)
    );
    assert_eq!(
        execute(&mut vm, "scoreboard players get #copy heap"),
        returned(true, 37)
    );
    assert_eq!(
        execute(&mut vm, "execute if data storage heap:stored {value:37}",),
        returned(true, 1)
    );
}

#[test]
fn in_memory_functions_admit_a_trailing_slash_identifier() {
    let mut vm = Vm::from_packs(
        [Pack::memory([
            function("example:leaf/", "return 23\n"),
            function("example:caller", "return run function example:leaf/\n"),
        ])],
        0,
    )
    .unwrap();

    for function in ["example:leaf/", "example:caller"] {
        assert_eq!(
            vm.execute_function(function, None, context(), LIMIT, drop)
                .unwrap(),
            returned(true, 23),
            "{function}"
        );
    }
}
