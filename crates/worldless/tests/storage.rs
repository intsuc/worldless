use worldless::{CompileError, ExecutionError, FunctionOutcome, Vm};

const LIMIT: usize = 512;

fn returned(success: bool, value: i32) -> FunctionOutcome {
    FunctionOutcome::Returned { success, value }
}

fn compile(functions: &[(&str, &str)]) -> Vm {
    Vm::from_functions(functions.iter().copied()).unwrap()
}

fn assert_function(vm: &mut Vm, function: &str, expected: FunctionOutcome) {
    assert_eq!(
        vm.execute_function(function, LIMIT).unwrap(),
        expected,
        "{function}"
    );
}

fn nested_compound(depth: usize) -> String {
    let mut value = String::with_capacity(depth * 4 + 1);
    for _ in 0..depth {
        value.push_str("{a:");
    }
    value.push('0');
    for _ in 0..depth {
        value.push('}');
    }
    value
}

#[test]
fn command_storage_is_persistent_namespaced_and_empty_when_missing() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:state {value:7}\ndata merge storage other:state {value:9}\ndata merge storage minecraft:defaulted {value:11}\n",
        ),
        (
            "example:read_example",
            "return run data get storage example:state value\n",
        ),
        (
            "example:read_other",
            "return run data get storage other:state value\n",
        ),
        (
            "example:read_default",
            "return run data get storage defaulted value\n",
        ),
        (
            "example:read_explicit_default",
            "return run data get storage :defaulted value\n",
        ),
        (
            "example:get_missing_root",
            "return run data get storage example:missing\n",
        ),
        (
            "example:get_missing_path",
            "return run data get storage example:missing value\n",
        ),
    ]);

    assert_function(&mut vm, "example:get_missing_root", returned(true, 1));
    assert_function(&mut vm, "example:get_missing_path", returned(false, 0));
    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:read_example", returned(true, 7));
    assert_function(&mut vm, "example:read_other", returned(true, 9));
    assert_function(&mut vm, "example:read_default", returned(true, 11));
    assert_function(&mut vm, "example:read_explicit_default", returned(true, 11));
}

#[test]
fn target_snbt_literals_and_data_get_preserve_types_and_java_values() {
    let mut vm = compile(&[
        (
            "example:setup",
            r#"data merge storage example:literals {byte:1b,short:2s,int:3i,long:4L,float:-1.25f,double:2.75d,huge_positive:1e300d,huge_negative:-1e300d,truth:true,falsehood:false,builtin_truth:bool(-2.5d),text:"A😀",escaped:"\x41\u0042\U0001F600",named:"\N{LATIN CAPITAL LETTER C}",list:[1,"x",{}],bytes:[B;1,-2],ints:[I;1b,2s,3],longs:[L;1b,2s,3,4L],uuid:uuid("00000000-0000-0000-0000-000000000001"),packed_uuid:uuid("12345678-9abc-def0-1357-2468ace0bdf1"),obj:{a:1,b:2},hex:0xffff_ffff,binary:0b1010,unsigned:255ub,unsigned_int:4294967295ui,unsigned_long:18446744073709551615ul,duplicate:1,duplicate:7}
"#,
        ),
        (
            "example:get_byte",
            "return run data get storage example:literals byte\n",
        ),
        (
            "example:get_short",
            "return run data get storage example:literals short\n",
        ),
        (
            "example:get_int",
            "return run data get storage example:literals int\n",
        ),
        (
            "example:get_long",
            "return run data get storage example:literals long\n",
        ),
        (
            "example:get_float",
            "return run data get storage example:literals float\n",
        ),
        (
            "example:get_double",
            "return run data get storage example:literals double\n",
        ),
        (
            "example:get_huge_positive",
            "return run data get storage example:literals huge_positive\n",
        ),
        (
            "example:get_huge_negative",
            "return run data get storage example:literals huge_negative\n",
        ),
        (
            "example:get_truth",
            "return run data get storage example:literals truth\n",
        ),
        (
            "example:get_falsehood",
            "return run data get storage example:literals falsehood\n",
        ),
        (
            "example:get_builtin_truth",
            "return run data get storage example:literals builtin_truth\n",
        ),
        (
            "example:get_text",
            "return run data get storage example:literals text\n",
        ),
        (
            "example:get_escaped",
            "return run data get storage example:literals escaped\n",
        ),
        (
            "example:get_named",
            "return run data get storage example:literals named\n",
        ),
        (
            "example:get_list",
            "return run data get storage example:literals list\n",
        ),
        (
            "example:get_bytes",
            "return run data get storage example:literals bytes\n",
        ),
        (
            "example:get_ints",
            "return run data get storage example:literals ints\n",
        ),
        (
            "example:get_longs",
            "return run data get storage example:literals longs\n",
        ),
        (
            "example:get_uuid",
            "return run data get storage example:literals uuid\n",
        ),
        (
            "example:get_uuid_tail",
            "return run data get storage example:literals uuid[-1]\n",
        ),
        (
            "example:get_obj",
            "return run data get storage example:literals obj\n",
        ),
        (
            "example:get_hex",
            "return run data get storage example:literals hex\n",
        ),
        (
            "example:get_binary",
            "return run data get storage example:literals binary\n",
        ),
        (
            "example:get_unsigned",
            "return run data get storage example:literals unsigned\n",
        ),
        (
            "example:get_unsigned_int",
            "return run data get storage example:literals unsigned_int\n",
        ),
        (
            "example:get_unsigned_long",
            "return run data get storage example:literals unsigned_long\n",
        ),
        (
            "example:get_duplicate",
            "return run data get storage example:literals duplicate\n",
        ),
        (
            "example:types_match",
            "return run execute if data storage example:literals {byte:1b,short:2s,int:3,long:4L,float:-1.25f,double:2.75d,truth:1b,falsehood:0b,builtin_truth:1b,packed_uuid:[I;0x12345678,0x9abcdef0,0x13572468,0xace0bdf1],hex:-1,unsigned:-1b,unsigned_int:-1,unsigned_long:-1L}\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    for (function, value) in [
        ("example:get_byte", 1),
        ("example:get_short", 2),
        ("example:get_int", 3),
        ("example:get_long", 4),
        ("example:get_float", -2),
        ("example:get_double", 2),
        ("example:get_huge_positive", i32::MAX),
        ("example:get_huge_negative", i32::MIN),
        ("example:get_truth", 1),
        ("example:get_builtin_truth", 1),
        ("example:get_text", 3),
        ("example:get_escaped", 4),
        ("example:get_named", 1),
        ("example:get_list", 3),
        ("example:get_bytes", 2),
        ("example:get_ints", 3),
        ("example:get_longs", 4),
        ("example:get_uuid", 4),
        ("example:get_uuid_tail", 1),
        ("example:get_obj", 2),
        ("example:get_hex", -1),
        ("example:get_binary", 10),
        ("example:get_unsigned", -1),
        ("example:get_unsigned_int", -1),
        ("example:get_unsigned_long", -1),
        ("example:get_duplicate", 7),
    ] {
        assert_function(&mut vm, function, returned(true, value));
    }
    assert_function(&mut vm, "example:get_falsehood", returned(true, 0));
    assert_function(&mut vm, "example:types_match", returned(true, 1));
}

#[test]
fn unicode_name_escapes_follow_java_se_25_character_names() {
    let mut vm = compile(&[
        (
            "example:setup",
            r#"data merge storage example:names {ordinary:"\N{ latin capital letter c }",control:"\N{BEL}",cjk:"\N{CJK UNIFIED IDEOGRAPHS 4E00}",hangul:"\N{HANGUL SYLLABLES AC00}",tangut:"\N{TANGUT 17000}",private:"\N{PRIVATE USE AREA E000}",surrogate:"\N{HIGH SURROGATES D800}",unnamed_control:"\N{LATIN 1 SUPPLEMENT 84}"}
"#,
        ),
        (
            "example:exact_values",
            r#"return run execute if data storage example:names {ordinary:"C",control:"\x07",cjk:"一",hangul:"가",tangut:"\U00017000",private:"\uE000",surrogate:"\uD800",unnamed_control:"\x84"}
"#,
        ),
        (
            "example:tangut_length",
            "return run data get storage example:names tangut\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:exact_values", returned(true, 1));
    assert_function(&mut vm, "example:tangut_length", returned(true, 2));

    for name in [
        "HANGUL SYLLABLE GA",
        "CJK UNIFIED IDEOGRAPH-4E00",
        "LINE FEED",
        "NUL",
        "CJK UNIFIED IDEOGRAPHS 04E00",
        "CJK UNIFIED IDEOGRAPHS EXTENSION B 2A6E0",
        "BASIC LATIN 41",
    ] {
        let source = format!(r#"data merge storage example:invalid {{value:"\N{{{name}}}"}}"#);
        assert!(matches!(
            Vm::from_functions([("example:invalid", source)]),
            Err(CompileError::InvalidFunction { .. })
        ));
    }
}

#[test]
fn data_get_and_nbt_paths_match_java_selection_rules() {
    let mut vm = compile(&[
        (
            "example:setup",
            r#"data merge storage example:paths {"a.b":7,nested:{value:8},items:[{kind:"x",value:1},{kind:"y",value:2},{kind:"x",value:3}],typed:{value:1b},empty_text:"",empty_list:[],fraction:-3.0d}
"#,
        ),
        (
            "example:quoted_key",
            "return run data get storage example:paths \"a.b\"\n",
        ),
        (
            "example:dotted_key",
            "return run data get storage example:paths a.b\n",
        ),
        (
            "example:nested",
            "return run data get storage example:paths nested.value\n",
        ),
        (
            "example:first_index",
            "return run data get storage example:paths items[0].value\n",
        ),
        (
            "example:last_index",
            "return run data get storage example:paths items[-1].value\n",
        ),
        (
            "example:all_elements",
            "return run execute if data storage example:paths items[]\n",
        ),
        (
            "example:matching_elements",
            "return run execute if data storage example:paths items[{kind:\"x\"}]\n",
        ),
        (
            "example:no_matching_elements",
            "return run execute if data storage example:paths items[{kind:\"missing\"}]\n",
        ),
        (
            "example:unless_no_match",
            "return run execute unless data storage example:paths items[{kind:\"missing\"}]\n",
        ),
        (
            "example:root_pattern",
            "return run execute if data storage example:paths {items:[{kind:\"y\"}]}\n",
        ),
        (
            "example:named_pattern",
            "return run execute if data storage example:paths typed{value:1b}\n",
        ),
        (
            "example:type_mismatch",
            "return run execute if data storage example:paths typed{value:1}\n",
        ),
        (
            "example:multiple_get",
            "return run data get storage example:paths items[].value\n",
        ),
        (
            "example:scaled_negative",
            "return run data get storage example:paths fraction 0.5\n",
        ),
        (
            "example:non_numeric_scale",
            "return run data get storage example:paths nested 2\n",
        ),
        (
            "example:empty_text",
            "return run data get storage example:paths empty_text\n",
        ),
        (
            "example:empty_list",
            "return run data get storage example:paths empty_list\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:quoted_key", returned(true, 7));
    assert_function(&mut vm, "example:dotted_key", returned(false, 0));
    assert_function(&mut vm, "example:nested", returned(true, 8));
    assert_function(&mut vm, "example:first_index", returned(true, 1));
    assert_function(&mut vm, "example:last_index", returned(true, 3));
    assert_function(&mut vm, "example:all_elements", returned(true, 3));
    assert_function(&mut vm, "example:matching_elements", returned(true, 2));
    assert_function(&mut vm, "example:no_matching_elements", returned(false, 0));
    assert_function(&mut vm, "example:unless_no_match", returned(true, 1));
    assert_function(&mut vm, "example:root_pattern", returned(true, 1));
    assert_function(&mut vm, "example:named_pattern", returned(true, 1));
    assert_function(&mut vm, "example:type_mismatch", returned(false, 0));
    assert_function(&mut vm, "example:multiple_get", returned(false, 0));
    assert_function(&mut vm, "example:scaled_negative", returned(true, -2));
    assert_function(&mut vm, "example:non_numeric_scale", returned(false, 0));
    assert_function(&mut vm, "example:empty_text", returned(true, 0));
    assert_function(&mut vm, "example:empty_list", returned(true, 0));
}

#[test]
fn data_merge_and_remove_report_only_real_changes() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:changes {nested:{keep:1,replace:2},scalar:1,number:1,list:[1,2,3],objects:[{drop:1},{drop:2},{keep:3}]}\n",
        ),
        (
            "example:merge",
            "return run data merge storage example:changes {nested:{replace:3,add:4},scalar:{inside:5}}\n",
        ),
        (
            "example:merge_again",
            "return run data merge storage example:changes {nested:{replace:3,add:4},scalar:{inside:5}}\n",
        ),
        (
            "example:merge_empty",
            "return run data merge storage example:changes {}\n",
        ),
        (
            "example:change_numeric_type",
            "return run data merge storage example:changes {number:1b}\n",
        ),
        (
            "example:same_numeric_type",
            "return run data merge storage example:changes {number:1b}\n",
        ),
        (
            "example:merged_shape",
            "return run execute if data storage example:changes {nested:{keep:1,replace:3,add:4},scalar:{inside:5},number:1b}\n",
        ),
        (
            "example:remove_object_fields",
            "return run data remove storage example:changes objects[].drop\n",
        ),
        (
            "example:remove_last",
            "return run data remove storage example:changes list[-1]\n",
        ),
        (
            "example:remove_rest",
            "return run data remove storage example:changes list[]\n",
        ),
        (
            "example:get_empty_list",
            "return run data get storage example:changes list\n",
        ),
        (
            "example:remove_missing",
            "return run data remove storage example:changes missing\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:merge", returned(true, 1));
    assert_function(&mut vm, "example:merge_again", returned(false, 0));
    assert_function(&mut vm, "example:merge_empty", returned(false, 0));
    assert_function(&mut vm, "example:change_numeric_type", returned(true, 1));
    assert_function(&mut vm, "example:same_numeric_type", returned(false, 0));
    assert_function(&mut vm, "example:merged_shape", returned(true, 1));
    assert_function(&mut vm, "example:remove_object_fields", returned(true, 2));
    assert_function(&mut vm, "example:remove_last", returned(true, 1));
    assert_function(&mut vm, "example:remove_rest", returned(true, 2));
    assert_function(&mut vm, "example:get_empty_list", returned(true, 0));
    assert_function(&mut vm, "example:remove_missing", returned(false, 0));

    let allowed_source = format!(
        "return run data merge storage example:allowed {}\n",
        nested_compound(511)
    );
    let rejected_source = format!(
        "return run data merge storage example:rejected {}\n",
        nested_compound(512)
    );
    let mut depth_vm = Vm::from_functions([
        ("example:allowed", allowed_source.as_str()),
        ("example:rejected", rejected_source.as_str()),
    ])
    .unwrap();
    assert_function(&mut depth_vm, "example:allowed", returned(true, 1));
    assert_function(&mut depth_vm, "example:rejected", returned(false, 0));
}

#[test]
fn data_modify_set_and_from_create_paths_and_count_changed_targets() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:source {values:[1,2,3],whole:{copied:4},texts:[\"first\",\"last\"]}\ndata merge storage example:target {items:[{value:1},{value:1},{value:2}]}\n",
        ),
        (
            "example:create_path",
            "return run data modify storage example:target created.deep set value 7\n",
        ),
        (
            "example:create_path_again",
            "return run data modify storage example:target created.deep set value 7\n",
        ),
        (
            "example:set_all",
            "return run data modify storage example:target items[].value set value 9\n",
        ),
        (
            "example:set_all_again",
            "return run data modify storage example:target items[].value set value 9\n",
        ),
        (
            "example:set_last_source",
            "return run data modify storage example:target picked set from storage example:source values[]\n",
        ),
        (
            "example:set_source_path",
            "return run data modify storage example:target copied set from storage example:source whole\n",
        ),
        (
            "example:set_source_root",
            "return run data modify storage example:target root_copy set from storage example:source\n",
        ),
        (
            "example:missing_source_path",
            "return run data modify storage example:target should_not_exist set from storage example:source missing\n",
        ),
        (
            "example:read_created",
            "return run data get storage example:target created.deep\n",
        ),
        (
            "example:read_picked",
            "return run data get storage example:target picked\n",
        ),
        (
            "example:read_copied",
            "return run data get storage example:target copied.copied\n",
        ),
        (
            "example:read_root_copy",
            "return run data get storage example:target root_copy\n",
        ),
        (
            "example:read_missing_target",
            "return run data get storage example:target should_not_exist\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:create_path", returned(true, 1));
    assert_function(&mut vm, "example:create_path_again", returned(false, 0));
    assert_function(&mut vm, "example:set_all", returned(true, 3));
    assert_function(&mut vm, "example:set_all_again", returned(false, 0));
    assert_function(&mut vm, "example:set_last_source", returned(true, 1));
    assert_function(&mut vm, "example:set_source_path", returned(true, 1));
    assert_function(&mut vm, "example:set_source_root", returned(true, 1));
    assert_function(&mut vm, "example:missing_source_path", returned(false, 0));
    assert_function(&mut vm, "example:read_created", returned(true, 7));
    assert_function(&mut vm, "example:read_picked", returned(true, 3));
    assert_function(&mut vm, "example:read_copied", returned(true, 4));
    assert_function(&mut vm, "example:read_root_copy", returned(true, 3));
    assert_function(&mut vm, "example:read_missing_target", returned(false, 0));
}

#[test]
fn data_modify_insert_prepend_and_append_preserve_order_and_collection_rules() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:source {values:[1,2]}\ndata merge storage example:append {list:[0]}\ndata merge storage example:prepend {list:[0]}\ndata merge storage example:insert {list:[0,3]}\ndata merge storage example:negative {list:[0]}\ndata merge storage example:invalid {list:[0,3]}\ndata merge storage example:multi {lists:[[0],[3]]}\ndata merge storage example:array {values:[B;1,2]}\n",
        ),
        (
            "example:append",
            "return run data modify storage example:append list append from storage example:source values[]\n",
        ),
        (
            "example:prepend",
            "return run data modify storage example:prepend list prepend from storage example:source values[]\n",
        ),
        (
            "example:insert",
            "return run data modify storage example:insert list insert 1 from storage example:source values[]\n",
        ),
        (
            "example:negative_append",
            "return run data modify storage example:negative list insert -1 value 4\n",
        ),
        (
            "example:negative_prepend",
            "return run data modify storage example:negative list insert -3 value 5\n",
        ),
        (
            "example:invalid_index",
            "return run data modify storage example:invalid list insert 99 value 7\n",
        ),
        (
            "example:multiple_targets",
            "return run data modify storage example:multi lists[] append value 9\n",
        ),
        (
            "example:append_numeric_to_array",
            "return run data modify storage example:array values append value 258\n",
        ),
        (
            "example:append_string_to_array",
            "return run data modify storage example:array values append value \"bad\"\n",
        ),
        (
            "example:append_first",
            "return run data get storage example:append list[0]\n",
        ),
        (
            "example:append_second",
            "return run data get storage example:append list[1]\n",
        ),
        (
            "example:append_third",
            "return run data get storage example:append list[2]\n",
        ),
        (
            "example:prepend_first",
            "return run data get storage example:prepend list[0]\n",
        ),
        (
            "example:prepend_second",
            "return run data get storage example:prepend list[1]\n",
        ),
        (
            "example:prepend_third",
            "return run data get storage example:prepend list[2]\n",
        ),
        (
            "example:insert_0",
            "return run data get storage example:insert list[0]\n",
        ),
        (
            "example:insert_1",
            "return run data get storage example:insert list[1]\n",
        ),
        (
            "example:insert_2",
            "return run data get storage example:insert list[2]\n",
        ),
        (
            "example:insert_3",
            "return run data get storage example:insert list[3]\n",
        ),
        (
            "example:negative_0",
            "return run data get storage example:negative list[0]\n",
        ),
        (
            "example:negative_1",
            "return run data get storage example:negative list[1]\n",
        ),
        (
            "example:negative_2",
            "return run data get storage example:negative list[2]\n",
        ),
        (
            "example:invalid_shape",
            "return run execute if data storage example:invalid {list:[0,3]}\n",
        ),
        (
            "example:invalid_size",
            "return run data get storage example:invalid list\n",
        ),
        (
            "example:multi_0",
            "return run data get storage example:multi lists[0][-1]\n",
        ),
        (
            "example:multi_1",
            "return run data get storage example:multi lists[1][-1]\n",
        ),
        (
            "example:array_shape",
            "return run execute if data storage example:array {values:[B;1,2,2]}\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:append", returned(true, 1));
    assert_function(&mut vm, "example:prepend", returned(true, 1));
    assert_function(&mut vm, "example:insert", returned(true, 1));
    assert_function(&mut vm, "example:negative_append", returned(true, 1));
    assert_function(&mut vm, "example:negative_prepend", returned(true, 1));
    assert_function(&mut vm, "example:invalid_index", returned(false, 0));
    assert_function(&mut vm, "example:multiple_targets", returned(true, 2));
    assert_function(
        &mut vm,
        "example:append_numeric_to_array",
        returned(true, 1),
    );
    assert_function(
        &mut vm,
        "example:append_string_to_array",
        returned(false, 0),
    );
    for (function, value) in [
        ("example:append_first", 0),
        ("example:append_second", 1),
        ("example:append_third", 2),
        ("example:prepend_first", 1),
        ("example:prepend_second", 2),
        ("example:prepend_third", 0),
    ] {
        assert_function(&mut vm, function, returned(true, value));
    }
    for (function, value) in [
        ("example:insert_0", 0),
        ("example:insert_1", 1),
        ("example:insert_2", 2),
        ("example:insert_3", 3),
        ("example:negative_0", 5),
        ("example:negative_1", 0),
        ("example:negative_2", 4),
        ("example:multi_0", 9),
        ("example:multi_1", 9),
    ] {
        assert_function(&mut vm, function, returned(true, value));
    }
    assert_function(&mut vm, "example:invalid_shape", returned(true, 1));
    assert_function(&mut vm, "example:invalid_size", returned(true, 2));
    assert_function(&mut vm, "example:array_shape", returned(true, 1));
}

#[test]
fn data_modify_merge_and_string_follow_source_and_utf16_rules() {
    let mut vm = compile(&[
        (
            "example:setup",
            r#"data merge storage example:string_source {objects:[{a:1,nested:{x:1}},{b:2,nested:{y:2}}],text:"A😀BC",integer:4,long:4L,float:1.0f,bad:{x:1}}
data merge storage example:string_target {object:{base:0,nested:{z:3}},objects:[{value:0},{value:0}]}
"#,
        ),
        (
            "example:merge_sources",
            "return run data modify storage example:string_target object merge from storage example:string_source objects[]\n",
        ),
        (
            "example:merge_targets",
            "return run data modify storage example:string_target objects[] merge value {added:1}\n",
        ),
        (
            "example:merge_non_compound",
            "return run data modify storage example:string_target object merge value 7\n",
        ),
        (
            "example:copy_string",
            "return run data modify storage example:string_target copied set string storage example:string_source text\n",
        ),
        (
            "example:slice_string",
            "return run data modify storage example:string_target sliced set string storage example:string_source text 1 -1\n",
        ),
        (
            "example:negative_slice",
            "return run data modify storage example:string_target tail set string storage example:string_source text -2\n",
        ),
        (
            "example:half_surrogate",
            "return run data modify storage example:string_target half set string storage example:string_source text 1 2\n",
        ),
        (
            "example:stringify_integer",
            "return run data modify storage example:string_target integer_text set string storage example:string_source integer\n",
        ),
        (
            "example:stringify_long",
            "return run data modify storage example:string_target long_text set string storage example:string_source long\n",
        ),
        (
            "example:stringify_float",
            "return run data modify storage example:string_target float_text set string storage example:string_source float\n",
        ),
        (
            "example:invalid_substring",
            "return run data modify storage example:string_target invalid set string storage example:string_source text 4 2\n",
        ),
        (
            "example:stringify_compound",
            "return run data modify storage example:string_target invalid_compound set string storage example:string_source bad\n",
        ),
        (
            "example:stringify_root",
            "return run data modify storage example:string_target invalid_root set string storage example:string_source\n",
        ),
        (
            "example:merged_nested_size",
            "return run data get storage example:string_target object.nested\n",
        ),
        (
            "example:merged_targets",
            "return run execute if data storage example:string_target objects[{added:1}]\n",
        ),
        (
            "example:copied_length",
            "return run data get storage example:string_target copied\n",
        ),
        (
            "example:sliced_length",
            "return run data get storage example:string_target sliced\n",
        ),
        (
            "example:tail_length",
            "return run data get storage example:string_target tail\n",
        ),
        (
            "example:half_length",
            "return run data get storage example:string_target half\n",
        ),
        (
            "example:exact_strings",
            r#"return run execute if data storage example:string_target {integer_text:"4",long_text:"4L",float_text:"1.0f",half:"\uD83D"}
"#,
        ),
        (
            "example:invalid_was_not_created",
            "return run execute unless data storage example:string_target invalid\n",
        ),
        (
            "example:invalid_compound_was_not_created",
            "return run execute unless data storage example:string_target invalid_compound\n",
        ),
        (
            "example:invalid_root_was_not_created",
            "return run execute unless data storage example:string_target invalid_root\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:merge_sources", returned(true, 1));
    assert_function(&mut vm, "example:merge_targets", returned(true, 2));
    assert_function(&mut vm, "example:merge_non_compound", returned(false, 0));
    for function in [
        "example:copy_string",
        "example:slice_string",
        "example:negative_slice",
        "example:half_surrogate",
        "example:stringify_integer",
        "example:stringify_long",
        "example:stringify_float",
    ] {
        assert_function(&mut vm, function, returned(true, 1));
    }
    assert_function(&mut vm, "example:invalid_substring", returned(false, 0));
    assert_function(&mut vm, "example:stringify_compound", returned(false, 0));
    assert_function(&mut vm, "example:stringify_root", returned(false, 0));
    assert_function(&mut vm, "example:merged_nested_size", returned(true, 3));
    assert_function(&mut vm, "example:merged_targets", returned(true, 2));
    assert_function(&mut vm, "example:copied_length", returned(true, 5));
    assert_function(&mut vm, "example:sliced_length", returned(true, 3));
    assert_function(&mut vm, "example:tail_length", returned(true, 2));
    assert_function(&mut vm, "example:half_length", returned(true, 1));
    assert_function(&mut vm, "example:exact_strings", returned(true, 1));
    assert_function(
        &mut vm,
        "example:invalid_was_not_created",
        returned(true, 1),
    );
    assert_function(
        &mut vm,
        "example:invalid_compound_was_not_created",
        returned(true, 1),
    );
    assert_function(
        &mut vm,
        "example:invalid_root_was_not_created",
        returned(true, 1),
    );
}

#[test]
fn data_conditions_chain_and_publish_counts_to_result_consumers() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:conditions {items:[1,2,3],flag:1}\nscoreboard objectives add state dummy\n",
        ),
        (
            "example:passing_chain",
            "return run execute if data storage example:conditions flag unless data storage example:conditions missing run return 9\n",
        ),
        (
            "example:failing_if",
            "return run execute if data storage example:conditions missing run return 9\n",
        ),
        (
            "example:failing_unless",
            "return run execute unless data storage example:conditions flag run return 9\n",
        ),
        (
            "example:store_count_in_score",
            "execute store result score #count state if data storage example:conditions items[]\nreturn run scoreboard players get #count state\n",
        ),
        (
            "example:store_failed_success_in_score",
            "execute store success score #success state if data storage example:conditions missing\nreturn run scoreboard players get #success state\n",
        ),
        (
            "example:store_count_in_storage",
            "execute store result storage example:conditions captured int 1 if data storage example:conditions items[]\nreturn run data get storage example:conditions captured\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:passing_chain", returned(true, 9));
    assert_function(&mut vm, "example:failing_if", returned(false, 0));
    assert_function(&mut vm, "example:failing_unless", returned(false, 0));
    assert_function(&mut vm, "example:store_count_in_score", returned(true, 3));
    assert_function(
        &mut vm,
        "example:store_failed_success_in_score",
        returned(true, 0),
    );
    assert_function(&mut vm, "example:store_count_in_storage", returned(true, 3));
}

#[test]
fn execute_store_storage_uses_java_numeric_conversions_and_callback_order() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:stored {scalar:1,untouched:9}\n",
        ),
        (
            "example:store_byte",
            "return run execute store result storage example:stored byte byte 100 run return 3\n",
        ),
        (
            "example:store_short",
            "return run execute store result storage example:stored short short 10000 run return 7\n",
        ),
        (
            "example:store_int",
            "return run execute store result storage example:stored int int 0.5 run return -7\n",
        ),
        (
            "example:store_long",
            "return run execute store result storage example:stored long long 2 run return 2147483647\n",
        ),
        (
            "example:store_float",
            "return run execute store result storage example:stored float float 1 run return 16777217\n",
        ),
        (
            "example:store_double",
            "return run execute store result storage example:stored double double 0.5 run return 7\n",
        ),
        (
            "example:store_failed_success",
            "return run execute store success storage example:stored failed int -3 run return fail\n",
        ),
        (
            "example:store_passed_success",
            "return run execute store success storage example:stored passed byte 3 run return 99\n",
        ),
        (
            "example:result_then_success",
            "return run execute store result storage example:stored ordered int 1 store success storage example:stored ordered int 1 run return 7\n",
        ),
        (
            "example:success_then_result",
            "return run execute store success storage example:stored ordered int 1 store result storage example:stored ordered int 1 run return 7\n",
        ),
        (
            "example:invalid_path",
            "return run execute store result storage example:stored scalar.child.grandchild int 1 run return 7\n",
        ),
        ("example:fallthrough", "# no callback\n"),
        (
            "example:no_callback",
            "execute store result storage example:stored untouched int 1 run function example:fallthrough\nreturn run data get storage example:stored untouched\n",
        ),
        (
            "example:callback_after_command",
            "return run execute store result storage example:stored timing int 1 run data modify storage example:stored timing set value 9\n",
        ),
        (
            "example:stored_types",
            "return run execute if data storage example:stored {byte:44b,short:4464s,int:-3,long:4294967294L,float:16777216.0f,double:3.5d,failed:0,passed:3b}\n",
        ),
        (
            "example:read_ordered",
            "return run data get storage example:stored ordered\n",
        ),
        (
            "example:read_scalar",
            "return run data get storage example:stored scalar\n",
        ),
        (
            "example:read_timing",
            "return run data get storage example:stored timing\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    for (function, outcome) in [
        ("example:store_byte", returned(true, 3)),
        ("example:store_short", returned(true, 7)),
        ("example:store_int", returned(true, -7)),
        ("example:store_long", returned(true, i32::MAX)),
        ("example:store_float", returned(true, 16_777_217)),
        ("example:store_double", returned(true, 7)),
        ("example:store_failed_success", returned(false, 0)),
        ("example:store_passed_success", returned(true, 99)),
    ] {
        assert_function(&mut vm, function, outcome);
    }
    assert_function(&mut vm, "example:stored_types", returned(true, 1));
    assert_function(&mut vm, "example:result_then_success", returned(true, 7));
    assert_function(&mut vm, "example:read_ordered", returned(true, 1));
    assert_function(&mut vm, "example:success_then_result", returned(true, 7));
    assert_function(&mut vm, "example:read_ordered", returned(true, 7));
    assert_function(&mut vm, "example:invalid_path", returned(true, 7));
    assert_function(&mut vm, "example:read_scalar", returned(true, 1));
    assert_function(&mut vm, "example:no_callback", returned(true, 9));
    assert_function(&mut vm, "example:callback_after_command", returned(true, 1));
    assert_function(&mut vm, "example:read_timing", returned(true, 1));
}

#[test]
fn storage_side_effects_before_the_command_limit_are_not_rolled_back() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:quota {present:1}\n",
        ),
        (
            "example:modify_at_limit",
            "return run data modify storage example:quota modified set value 7\n",
        ),
        (
            "example:store_at_limit",
            "return run execute store result storage example:quota stored int 1 run return 6\n",
        ),
        (
            "example:condition_at_limit",
            "return run execute if data storage example:quota present run return 9\n",
        ),
        (
            "example:terminal_condition_at_limit",
            "return run execute if data storage example:quota present\n",
        ),
        (
            "example:read_modified",
            "return run data get storage example:quota modified\n",
        ),
        (
            "example:read_stored",
            "return run data get storage example:quota stored\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_eq!(
        vm.execute_function("example:modify_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_function(&mut vm, "example:read_modified", returned(true, 7));
    assert_eq!(
        vm.execute_function("example:store_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_function(&mut vm, "example:read_stored", returned(true, 6));
    assert_eq!(
        vm.execute_function("example:condition_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:condition_at_limit", 3)
            .unwrap(),
        returned(true, 9)
    );
    assert_eq!(
        vm.execute_function("example:terminal_condition_at_limit", 2),
        Err(ExecutionError::CommandLimitExceeded { limit: 2 })
    );
    assert_eq!(
        vm.execute_function("example:terminal_condition_at_limit", 3)
            .unwrap(),
        returned(true, 1)
    );
}

#[test]
fn failed_path_creation_observes_command_storage_aliasing() {
    let mut vm = compile(&[
        (
            "example:setup",
            "data merge storage example:existing {keep:1}\ndata merge storage example:store_error {keep:1}\ndata merge storage example:emptied {only:1}\n",
        ),
        (
            "example:fail_existing",
            "return run data modify storage example:existing a.b[0] set value 1\n",
        ),
        (
            "example:read_existing_partial",
            "return run data get storage example:existing a.b\n",
        ),
        (
            "example:fail_missing",
            "return run data modify storage example:missing a.b[0] set value 1\n",
        ),
        (
            "example:read_missing_partial",
            "return run data get storage example:missing a.b\n",
        ),
        (
            "example:store_count_zero",
            "return run execute store result storage example:store_missing a.b[0] int 1 run return 7\n",
        ),
        (
            "example:read_store_partial",
            "return run data get storage example:store_missing a.b\n",
        ),
        (
            "example:store_error_existing",
            "return run execute store result storage example:store_error a.b[0].c int 1 run return 7\n",
        ),
        (
            "example:read_store_error_partial",
            "return run data get storage example:store_error a.b\n",
        ),
        (
            "example:empty_storage",
            "return run data remove storage example:emptied only\n",
        ),
        (
            "example:fail_after_empty",
            "return run data modify storage example:emptied a.b[0] set value 1\n",
        ),
        (
            "example:read_after_empty_failure",
            "return run data get storage example:emptied a.b\n",
        ),
    ]);

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:fail_existing", returned(false, 0));
    assert_function(&mut vm, "example:read_existing_partial", returned(true, 0));
    assert_function(&mut vm, "example:fail_missing", returned(false, 0));
    assert_function(&mut vm, "example:read_missing_partial", returned(false, 0));
    assert_function(&mut vm, "example:store_count_zero", returned(true, 7));
    assert_function(&mut vm, "example:read_store_partial", returned(true, 0));
    assert_function(&mut vm, "example:store_error_existing", returned(true, 7));
    assert_function(
        &mut vm,
        "example:read_store_error_partial",
        returned(true, 0),
    );
    assert_function(&mut vm, "example:empty_storage", returned(true, 1));
    assert_function(&mut vm, "example:fail_after_empty", returned(false, 0));
    assert_function(
        &mut vm,
        "example:read_after_empty_failure",
        returned(false, 0),
    );
}

#[test]
fn rejects_invalid_and_out_of_slice_storage_command_forms() {
    for command in [
        "data get storage Upper:value",
        "data merge storage example:value 1",
        "data merge storage example:value {number:01}",
        "data merge storage example:value {value:bool(\"not numeric\")}",
        "data merge storage example:value {value:uuid(1)}",
        "data merge storage example:value {value:uuid(\"1-2-3-4-5\")}",
        "data merge storage example:value {value:uuid(\"+0000000-0000-0000-0000-000000000000\")}",
        "data modify storage example:value path set value [B;1s]",
        "data get storage example:value items[",
        "execute store result storage example:value path boolean 1 run return 1",
        "execute store result storage example:value path int run return 1",
        "data get entity @s",
        "data get block 0 0 0",
        "execute if data entity @s path run return 1",
        "execute store result entity @s path int 1 run return 1",
        "data modify storage example:value path set compute 1",
    ] {
        assert!(
            matches!(
                Vm::from_functions([
                    ("example:target", "# target\n"),
                    ("example:invalid", command),
                ]),
                Err(CompileError::InvalidFunction { .. })
            ),
            "{command:?}"
        );
    }
}
