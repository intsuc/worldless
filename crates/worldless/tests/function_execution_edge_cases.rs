mod common;

use common::context;
use worldless::{ExecutionError, ExecutionOutcome, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 1_024;

fn returned(value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result {
        success: true,
        value,
    }
}

fn compile(
    functions: impl IntoIterator<Item = (String, String)>,
    tags: impl IntoIterator<Item = (String, String)>,
) -> Vm {
    let functions = functions
        .into_iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, id, source));
    let tags = tags
        .into_iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::FunctionTag, id, source));
    Vm::from_packs([Pack::memory(functions.chain(tags))], 0).unwrap()
}

fn function(id: &str, source: &str) -> (String, String) {
    (id.to_owned(), source.to_owned())
}

#[test]
fn floating_point_counter_steps_at_both_valid_boundaries() {
    const LOWER: i32 = 1_073_741_760;
    const UPPER: i32 = 2_147_483_520;

    let mut vm = compile(
        [
            function(
                "example:set_lower",
                "data modify storage example:counter value set value 1073741760\n",
            ),
            function(
                "example:set_below_upper",
                "data modify storage example:counter value set value 2147483519\n",
            ),
            function(
                "example:increment",
                "execute store result storage example:counter value int 1.0000000009313226 run data get storage example:counter value 1.0\nreturn run data get storage example:counter value\n",
            ),
            function(
                "example:decrement",
                "execute store result storage example:counter value int 0.9999999999999999 run data get storage example:counter value 1.0\nreturn run data get storage example:counter value\n",
            ),
        ],
        [],
    );

    vm.execute_function("example:set_lower", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:increment", None, context(), LIMIT, drop)
            .unwrap(),
        returned(LOWER + 1)
    );
    assert_eq!(
        vm.execute_function("example:decrement", None, context(), LIMIT, drop)
            .unwrap(),
        returned(LOWER)
    );

    vm.execute_function("example:set_below_upper", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:increment", None, context(), LIMIT, drop)
            .unwrap(),
        returned(UPPER)
    );
    assert_eq!(
        vm.execute_function("example:decrement", None, context(), LIMIT, drop)
            .unwrap(),
        returned(UPPER - 1)
    );
}

#[test]
fn deep_list_mapped_trie_keeps_distinct_signed_index_leaves() {
    const DEPTH: usize = 32;

    let mut touch_body = String::from("scoreboard players operation #_index lmt = #index lmt\n");
    for depth in 0..DEPTH {
        let path = format!("tree{}", "[-2]".repeat(depth));
        touch_body.push_str(&format!(
            "execute store result score #size lmt if data storage example:lmt {path}[]\n\
             execute if score #size lmt matches 0 run data modify storage example:lmt {path} append from storage example:lmt branch[]\n\
             execute if score #size lmt matches 3 if score #index lmt matches 0.. run data remove storage example:lmt {path}[2]\n\
             execute if score #size lmt matches 2 if score #index lmt matches ..-1 run data modify storage example:lmt {path} append value []\n"
        ));
        if depth + 1 != DEPTH {
            touch_body.push_str("scoreboard players operation #index lmt += #index lmt\n");
        }
    }

    let leaf = format!("tree{}", "[-2]".repeat(DEPTH));
    let functions = vec![
        function(
            "example:setup",
            "data modify storage example:lmt branch set value [[],[]]\nscoreboard objectives add lmt dummy\n",
        ),
        function("example:touch_internal", &touch_body),
        function(
            "example:touch",
            "execute unless score #_index lmt = #index lmt run function example:touch_internal\n",
        ),
        (
            "example:insert_zero".to_owned(),
            format!(
                "scoreboard players set #index lmt 0\nfunction example:touch\ndata modify storage example:lmt {leaf} append value 0b\nreturn run data get storage example:lmt {leaf}[0]\n"
            ),
        ),
        (
            "example:insert_one".to_owned(),
            format!(
                "scoreboard players set #index lmt 1\nfunction example:touch\ndata modify storage example:lmt {leaf} append value {{data:1s}}\nreturn run data get storage example:lmt {leaf}[0].data\n"
            ),
        ),
        (
            "example:insert_negative_one".to_owned(),
            format!(
                "scoreboard players set #index lmt -1\nfunction example:touch\ndata modify storage example:lmt {leaf} append value 4294967295L\nreturn run data get storage example:lmt {leaf}[0]\n"
            ),
        ),
        (
            "example:read_zero".to_owned(),
            format!(
                "scoreboard players set #index lmt 0\nfunction example:touch\nreturn run data get storage example:lmt {leaf}[0]\n"
            ),
        ),
        (
            "example:read_one".to_owned(),
            format!(
                "scoreboard players set #index lmt 1\nfunction example:touch\nreturn run data get storage example:lmt {leaf}[0].data\n"
            ),
        ),
    ];
    let mut vm = compile(functions, []);

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("example:insert_zero", None, context(), LIMIT, drop)
            .unwrap(),
        returned(0)
    );
    assert_eq!(
        vm.execute_function("example:insert_one", None, context(), LIMIT, drop)
            .unwrap(),
        returned(1)
    );
    assert_eq!(
        vm.execute_function("example:insert_negative_one", None, context(), LIMIT, drop,)
            .unwrap(),
        returned(i32::MAX)
    );
    assert_eq!(
        vm.execute_function("example:read_zero", None, context(), LIMIT, drop)
            .unwrap(),
        returned(0)
    );
    assert_eq!(
        vm.execute_function("example:read_one", None, context(), LIMIT, drop)
            .unwrap(),
        returned(1)
    );
}

#[test]
fn recursive_first_tag_member_starves_its_tails_and_later_siblings() {
    let mut vm = compile(
        [
            function(
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #entered state 0\nscoreboard players set #tail state 0\nscoreboard players set #finally state 0\n",
            ),
            function(
                "example:recursive",
                "scoreboard players add #entered state 1\nfunction example:recursive\nscoreboard players set #tail state 1\n",
            ),
            function(
                "example:finally",
                "scoreboard players set #finally state 1\n",
            ),
            function(
                "example:check",
                "execute if score #entered state matches 1.. if score #tail state matches 0 if score #finally state matches 0 run return 1\nreturn 0\n",
            ),
        ],
        [function(
            "example:recursive_tag",
            r#"{"values":["example:recursive","example:finally"]}"#,
        )],
    );

    vm.execute_function("example:setup", None, context(), LIMIT, drop)
        .unwrap();
    assert_eq!(
        vm.execute_function("#example:recursive_tag", None, context(), 20, drop),
        Err(ExecutionError::CommandLimitExceeded { limit: 20 })
    );
    assert_eq!(
        vm.execute_function("example:check", None, context(), LIMIT, drop)
            .unwrap(),
        returned(1)
    );
}
