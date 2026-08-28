mod common;

use common::context;
use worldless::{ExecutionOutcome, FunctionArguments, MemoryResource, Pack, ResourceKind, Vm};

const LIMIT: usize = 512;

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn empty_vm() -> Vm {
    Vm::from_packs([Pack::memory(std::iter::empty::<MemoryResource>())], 0).unwrap()
}

fn execute(vm: &mut Vm, command: &str) -> ExecutionOutcome {
    vm.execute_command(command, context(), LIMIT, drop).unwrap()
}

fn initialize_storage(vm: &mut Vm, storage: &str, value: &str) {
    assert_eq!(
        execute(vm, &format!("data merge storage {storage} {value}")),
        returned(true, 1)
    );
}

fn assert_storage(vm: &mut Vm, storage: &str, expected: &str) {
    execute(vm, "data remove storage worldless_test:comparison actual");
    execute(vm, "data remove storage worldless_test:comparison expected");
    assert_eq!(
        execute(
            vm,
            &format!(
                "data modify storage worldless_test:comparison actual set from storage {storage}"
            ),
        ),
        returned(true, 1)
    );
    assert_eq!(
        execute(
            vm,
            &format!("data modify storage worldless_test:comparison expected set value {expected}"),
        ),
        returned(true, 1)
    );
    assert_eq!(
        execute(
            vm,
            "data modify storage worldless_test:comparison expected merge from storage worldless_test:comparison actual",
        ),
        returned(false, 0),
        "actual storage contains data absent from the expectation"
    );
    assert_eq!(
        execute(
            vm,
            "data modify storage worldless_test:comparison actual merge from storage worldless_test:comparison expected",
        ),
        returned(false, 0),
        "expected storage contains data absent from the actual value"
    );
}

#[test]
fn macro_classifier_distinguishes_all_nbt_tag_kinds() {
    let resources = [
        MemoryResource::new(
            ResourceKind::Function,
            "minecraft:get_id",
            r#"$execute if data storage $(accessor){} run return 10
data remove storage _ key
$execute store success storage _ is_primitive byte 1 run data modify storage _ key set string storage $(accessor)
$execute store success storage _ is_numeric byte 1 run data get storage $(accessor) 0
$execute if data storage _ {is_numeric:true} run data modify storage _ key set string storage $(accessor) -1
execute if data storage _ {is_numeric:true} run return run function get_id-suffix_to_id with storage _
execute if data storage _ {is_primitive:true} run return 8
$execute unless data storage $(accessor)[0] run data modify storage _ key set from storage $(accessor)
$execute unless data storage $(accessor)[0] run return run function get_id-empty_collection_to_id with storage _
$execute store success storage _ is_list byte 1 run data modify storage $(accessor) append value ""
$execute if data storage _ {is_list:true} run data remove storage $(accessor)[-1]
execute if data storage _ {is_list:true} run return 9
$data modify storage _ key set string storage $(accessor)[0] -1
return run function get_id-array_suffix_to_id with storage _
"#,
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "minecraft:get_id-suffix_to_id",
            r#"execute unless data storage _ suffix_to_id run data modify storage _ suffix_to_id set value {b:1,s:2,0:3,1:3,2:3,3:3,4:3,5:3,6:3,7:3,8:3,9:3,L:4,f:5,d:6}
$return run data get storage _ suffix_to_id.$(key)
"#,
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "minecraft:get_id-empty_collection_to_id",
            r#"execute unless data storage _ empty_collection_to_id run data modify storage _ empty_collection_to_id set value {"[B;]":7,"[]":9,"[I;]":11,"[L;]":12}
$return run data get storage _ empty_collection_to_id."$(key)"
"#,
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "minecraft:get_id-array_suffix_to_id",
            r#"execute unless data storage _ array_suffix_to_id run data modify storage _ array_suffix_to_id set value {b:7,0:11,1:11,2:11,3:11,4:11,5:11,6:11,7:11,8:11,9:11,L:12}
$return run data get storage _ array_suffix_to_id.$(key)
"#,
        ),
    ];
    let mut vm = Vm::from_packs([Pack::memory(resources)], 0).unwrap();
    assert_eq!(
        execute(
            &mut vm,
            r#"data merge storage example: {1:0b,2:0s,3:0,4:0L,5:0.0f,6:0.0d,7:[B;],8:"",9:[],10:{},11:[I;],12:[L;],7_:[B;0b],8_:"0b",9_:[0b],10_:{0:0b},11_:[I;0],12_:[L;0L]}"#,
        ),
        returned(true, 1)
    );

    for (key, expected) in [
        ("1", 1),
        ("2", 2),
        ("3", 3),
        ("4", 4),
        ("5", 5),
        ("6", 6),
        ("7", 7),
        ("8", 8),
        ("9", 9),
        ("10", 10),
        ("11", 11),
        ("12", 12),
        ("7_", 7),
        ("8_", 8),
        ("9_", 9),
        ("10_", 10),
        ("11_", 11),
        ("12_", 12),
    ] {
        let arguments =
            FunctionArguments::from_snbt(&format!(r#"{{accessor:"example: {key}"}}"#)).unwrap();
        assert_eq!(
            vm.execute_function("minecraft:get_id", Some(&arguments), context(), LIMIT, drop,)
                .unwrap(),
            returned(true, expected),
            "tag at key {key}"
        );
    }
}

#[test]
fn execute_store_match_creation_appends_after_overwriting_its_pattern() {
    let mut vm = empty_vm();
    let command = "execute store result storage _ _[{_:1}]._ int 0 if data storage _ _";

    assert_eq!(execute(&mut vm, command), returned(false, 0));
    assert_eq!(execute(&mut vm, command), returned(true, 1));
    assert_eq!(execute(&mut vm, command), returned(true, 1));
    assert_eq!(execute(&mut vm, "data get storage _ _"), returned(true, 3));
    for index in 0..3 {
        assert_eq!(
            execute(&mut vm, &format!("data get storage _ _[{index}]._")),
            returned(true, 0)
        );
    }
}

#[test]
fn nested_all_elements_create_and_replace_values() {
    let mut vm = empty_vm();
    initialize_storage(&mut vm, "example:q7a", r#"{a:[[]]}"#);
    initialize_storage(&mut vm, "example:q7b", r#"{a:[[0],[]]}"#);

    assert_eq!(
        execute(
            &mut vm,
            r#"data modify storage example:q7a a[][] set value "0""#,
        ),
        returned(true, 1)
    );
    assert_eq!(
        execute(
            &mut vm,
            r#"data modify storage example:q7b a[][] set value "0""#,
        ),
        returned(true, 2)
    );
    assert_storage(&mut vm, "example:q7a", r#"{a: [["0"]]}"#);
    assert_storage(&mut vm, "example:q7b", r#"{a: [["0"], ["0"]]}"#);
}

#[test]
fn multi_target_insert_keeps_changes_before_an_invalid_index() {
    let mut vm = empty_vm();
    initialize_storage(&mut vm, "example:q10", r#"{a:[["0"],["1"],[]]}"#);

    assert_eq!(
        execute(
            &mut vm,
            r#"data modify storage example:q10 a[] insert -2 value "0""#,
        ),
        returned(false, 0)
    );
    assert_storage(
        &mut vm,
        "example:q10",
        r#"{a: [["0", "0"], ["0", "1"], []]}"#,
    );
}

#[test]
fn aliased_append_snapshots_sources_and_accepts_heterogeneous_values() {
    let mut vm = empty_vm();
    initialize_storage(&mut vm, "example:q13a", r#"{a:[["0"],[]]}"#);
    initialize_storage(&mut vm, "example:q13b", r#"{a:[[],["0"]]}"#);

    for storage in ["example:q13a", "example:q13b"] {
        assert_eq!(
            execute(
                &mut vm,
                &format!("data modify storage {storage} a[] append from storage {storage} a[]"),
            ),
            returned(true, 2)
        );
    }
    assert_storage(
        &mut vm,
        "example:q13a",
        r#"{a: [["0", ["0"], []], [["0"], []]]}"#,
    );
    assert_storage(
        &mut vm,
        "example:q13b",
        r#"{a: [[[], ["0"]], ["0", [], ["0"]]]}"#,
    );
}

#[test]
fn matched_element_creation_uses_current_heterogeneous_list_behavior() {
    let mut vm = empty_vm();
    initialize_storage(&mut vm, "example:q15a", r#"{a:[[]]}"#);
    initialize_storage(&mut vm, "example:q15b", r#"{a:[[],[[]]]}"#);

    assert_eq!(
        execute(
            &mut vm,
            r#"data modify storage example:q15a a[][{b:"1"}].b set value "0""#,
        ),
        returned(true, 1)
    );
    assert_eq!(
        execute(
            &mut vm,
            r#"data modify storage example:q15b a[][{b:"1"}].b set value "0""#,
        ),
        returned(true, 2)
    );
    assert_storage(&mut vm, "example:q15a", r#"{a: [[{b: "0"}]]}"#);
    assert_storage(
        &mut vm,
        "example:q15b",
        r#"{a: [[{b: "0"}], [[], {b: "0"}]]}"#,
    );
}

#[test]
fn stacked_stores_apply_in_order_after_the_source_command_fails() {
    for (suffix, initial, expected) in [
        ("a", "{a:[[[[0]]]]}", "{a: [[[[0]]]]}"),
        ("b", "{a:[[[[0]]],[[[0]]]]}", "{a: [[[0]], [[0]]]}"),
        ("c", "{a:[[[[0]]],[[[0]]],[[[0]]]]}", "{a: [[0], [0], [0]]}"),
        ("d", "{a:[[[[0]]],[[[0]]],[[[1]]]]}", "{a: [0, 0, 0]}"),
    ] {
        let storage = format!("example:q17{suffix}");
        let mut vm = empty_vm();
        initialize_storage(&mut vm, &storage, initial);
        let command = format!(
            "execute store result storage {storage} {{a:[[[[1]]]]}}.a[] int 0 \
             store result storage {storage} a[2][] int 0 \
             store result storage {storage} {{a:[[0]]}}.a[][] int 0 \
             store result storage {storage} a[1][][] int 0 \
             store result storage {storage} {{a:[[[0]]]}}.a[][][] int 0 \
             run data get storage {storage} b"
        );

        assert_eq!(execute(&mut vm, &command), returned(false, 0), "{suffix}");
        assert_storage(&mut vm, &storage, expected);
    }
}
