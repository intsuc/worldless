use worldless::{
    ExecutionContext, ExecutionError, ExecutionOutcome, MemoryResource, Pack, Position,
    ResourceKind, Rotation, Vm,
};

const LIMIT: usize = 256;

fn function(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::Function, id, source)
}

fn predicate(id: &str, source: impl Into<String>) -> MemoryResource {
    MemoryResource::new(ResourceKind::Predicate, id, source)
}

fn number_provider(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::NumberProvider, id, source)
}

fn function_tag(id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(ResourceKind::FunctionTag, id, source)
}

fn location(id: &str, x: &str, y: &str, z: &str) -> MemoryResource {
    let source =
        r#"{"type":"minecraft:location_check","predicate":{"position":{"x":$X,"y":$Y,"z":$Z}}}"#
            .replace("$X", x)
            .replace("$Y", y)
            .replace("$Z", z);
    predicate(id, source)
}

fn compile(resources: impl IntoIterator<Item = MemoryResource>) -> Vm {
    Vm::from_packs([Pack::memory(resources)], 0).unwrap()
}

fn context(x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> ExecutionContext {
    ExecutionContext::new(Position::new(x, y, z), Rotation::new(yaw, pitch))
}

fn returned(value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result {
        success: true,
        value,
    }
}

fn execute(vm: &mut Vm, id: &str, initial: ExecutionContext) -> ExecutionOutcome {
    vm.execute_function(id, None, initial, LIMIT).unwrap()
}

#[test]
fn absolute_relative_ordered_and_offset_positions_are_observable() {
    let mut vm = compile([
        function(
            "example:absolute",
            "return run execute positioned 1 2 3 if predicate example:absolute run return 11\n",
        ),
        function(
            "example:relative",
            "return run execute positioned ~1 ~-2 ~0.25 if predicate example:relative run return 12\n",
        ),
        function(
            "example:ordered",
            "return run execute positioned 1 2 3 positioned ~1 ~2 ~3 if predicate example:ordered run return 13\n",
        ),
        function(
            "example:offset",
            "return run execute positioned 1 2 3 if predicate example:offset run return 14\n",
        ),
        location("example:absolute", "1.5", "2", "3.5"),
        location("example:relative", "11", "18", "30.25"),
        location("example:ordered", "2.5", "4", "6.5"),
        predicate(
            "example:offset",
            r#"{"type":"minecraft:location_check","offsetX":2,"offsetY":-2,"offsetZ":3,"predicate":{"position":{"x":3.5,"y":0,"z":6.5}}}"#,
        ),
    ]);

    let arbitrary = context(-100.0, -200.0, -300.0, 37.0, -12.0);
    assert_eq!(
        execute(&mut vm, "example:absolute", arbitrary),
        returned(11)
    );
    assert_eq!(
        execute(
            &mut vm,
            "example:relative",
            context(10.0, 20.0, 30.0, 0.0, 0.0),
        ),
        returned(12)
    );
    assert_eq!(execute(&mut vm, "example:ordered", arbitrary), returned(13));
    assert_eq!(execute(&mut vm, "example:offset", arbitrary), returned(14));
}

#[test]
fn local_coordinates_use_the_current_rotation_and_modifier_order() {
    let near_zero = r#"{"min":-0.000001,"max":0.000001}"#;
    let near_one = r#"{"min":0.999999,"max":1.000001}"#;
    let near_negative_one = r#"{"min":-1.000001,"max":-0.999999}"#;
    let mut vm = compile([
        function(
            "example:local",
            "return run execute positioned ^1 ^2 ^3 if predicate example:local run return 21\n",
        ),
        function(
            "example:local_before_rotation",
            "return run execute positioned ^ ^ ^1 rotated 90 0 if predicate example:forward run return 22\n",
        ),
        function(
            "example:local_after_rotation",
            "return run execute rotated 90 0 positioned ^ ^ ^1 if predicate example:left run return 23\n",
        ),
        location("example:local", "11", "22", "33"),
        location("example:forward", near_zero, near_zero, near_one),
        location("example:left", near_negative_one, near_zero, near_zero),
    ]);

    assert_eq!(
        execute(
            &mut vm,
            "example:local",
            context(10.0, 20.0, 30.0, 0.0, 0.0),
        ),
        returned(21)
    );
    assert_eq!(
        execute(
            &mut vm,
            "example:local_before_rotation",
            context(0.0, 0.0, 0.0, 0.0, 0.0),
        ),
        returned(22)
    );
    assert_eq!(
        execute(
            &mut vm,
            "example:local_after_rotation",
            context(0.0, 0.0, 0.0, 0.0, 0.0),
        ),
        returned(23)
    );
}

#[test]
fn facing_align_and_entityless_anchors_match_command_source_semantics() {
    let near_zero = r#"{"min":-0.000001,"max":0.000001}"#;
    let near_one = r#"{"min":0.999999,"max":1.000001}"#;
    let mut vm = compile([
        function(
            "example:facing",
            "return run execute facing ~1 ~ ~ positioned ^ ^ ^1 if predicate example:east run return 31\n",
        ),
        function(
            "example:align",
            "return run execute align xz if predicate example:aligned run return 32\n",
        ),
        function(
            "example:saturating_align",
            "return run execute align x if predicate example:saturated run return 33\n",
        ),
        function(
            "example:feet",
            "return run execute anchored feet positioned ^ ^ ^1 if predicate example:forward run return 34\n",
        ),
        function(
            "example:eyes",
            "return run execute anchored eyes positioned ^ ^ ^1 if predicate example:forward run return 35\n",
        ),
        location("example:east", near_one, near_zero, near_zero),
        location("example:aligned", "1", "-1.25", "3"),
        location("example:saturated", "2147483647", "0", "0"),
        location("example:forward", near_zero, near_zero, near_one),
    ]);

    assert_eq!(
        execute(
            &mut vm,
            "example:facing",
            context(0.0, 0.0, 0.0, 123.0, 45.0),
        ),
        returned(31)
    );
    assert_eq!(
        execute(
            &mut vm,
            "example:align",
            context(1.75, -1.25, 3.9, 0.0, 0.0),
        ),
        returned(32)
    );
    assert_eq!(
        execute(
            &mut vm,
            "example:saturating_align",
            context(f64::MAX, 0.0, 0.0, 0.0, 0.0),
        ),
        returned(33)
    );
    let entityless = context(0.0, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(execute(&mut vm, "example:feet", entityless), returned(34));
    assert_eq!(execute(&mut vm, "example:eyes", entityless), returned(35));
}

#[test]
fn transformed_context_is_inherited_by_children_but_not_the_callers_next_line() {
    let mut vm = compile([
        function(
            "example:child",
            "execute if predicate example:transformed run return 41\nreturn fail\n",
        ),
        function(
            "example:inherit",
            "return run execute positioned 5.0 6.0 7.0 run function example:child\n",
        ),
        function(
            "example:no_leak",
            "execute positioned 5.0 6.0 7.0 if predicate example:transformed\nexecute if predicate example:initial run return 42\nreturn fail\n",
        ),
        function(
            "example:function_condition",
            "return run execute positioned 5.0 6.0 7.0 if function example:child run return 43\n",
        ),
        function(
            "example:tag",
            "scoreboard objectives add state dummy\nexecute positioned 5.0 6.0 7.0 run function #example:children\nreturn run scoreboard players get #observed state\n",
        ),
        function(
            "example:tag_first",
            "execute if predicate example:transformed run scoreboard players set #observed state 44\n",
        ),
        function(
            "example:tag_second",
            "execute if predicate example:transformed run scoreboard players add #observed state 1\n",
        ),
        function_tag(
            "example:children",
            r#"{"values":["example:tag_first","example:tag_second"]}"#,
        ),
        location("example:transformed", "5", "6", "7"),
        location("example:initial", "1", "2", "3"),
    ]);
    let initial = context(1.0, 2.0, 3.0, 0.0, 0.0);

    assert_eq!(execute(&mut vm, "example:inherit", initial), returned(41));
    assert_eq!(execute(&mut vm, "example:no_leak", initial), returned(42));
    assert_eq!(
        execute(&mut vm, "example:function_condition", initial),
        returned(43)
    );
    assert_eq!(execute(&mut vm, "example:tag", initial), returned(45));
}

#[test]
fn transformed_context_reaches_number_providers_in_compute_and_data_compute() {
    let mut vm = compile([
        function(
            "example:compute",
            "return run execute positioned 5.0 6.0 7.0 run compute default example:contextual integer\n",
        ),
        function(
            "example:data_compute",
            "data merge storage example:state {value:0}\nexecute positioned 5.0 6.0 7.0 run data modify storage example:state value set compute default example:contextual integer\nreturn run data get storage example:state value\n",
        ),
        number_provider(
            "example:contextual",
            r#"{"type":"conditional","condition":"example:transformed","on_true":71,"on_false":72}"#,
        ),
        location("example:transformed", "5", "6", "7"),
    ]);
    let initial = context(1.0, 2.0, 3.0, 0.0, 0.0);

    assert_eq!(execute(&mut vm, "example:compute", initial), returned(71));
    assert_eq!(
        execute(&mut vm, "example:data_compute", initial),
        returned(71)
    );
}

#[test]
fn macro_coordinates_are_resolved_against_the_invocation_context() {
    let mut vm = compile([
        function(
            "example:macro",
            "$execute positioned ~$(x) ~$(y) ~$(z) if predicate example:macro_position run return $(value)\nreturn fail\n",
        ),
        function(
            "example:invoke",
            "return run function example:macro {x:1,y:-2,z:3,value:51}\n",
        ),
        location("example:macro_position", "11", "18", "33"),
    ]);

    assert_eq!(
        execute(
            &mut vm,
            "example:invoke",
            context(10.0, 20.0, 30.0, 0.0, 0.0),
        ),
        returned(51)
    );
}

#[test]
fn context_redirects_consume_quota_even_when_the_chain_is_inactive() {
    let mut vm = compile([
        function(
            "example:all_transforms",
            "return run execute positioned ~ ~ ~ rotated ~ ~ facing ~ ~ ~ align xyz anchored feet run return 61\n",
        ),
        function(
            "example:inactive",
            "return run execute if predicate example:false positioned ~ ~ ~ anchored eyes run return 62\n",
        ),
        predicate("example:false", r#"{"type":"minecraft:any_of","terms":[]}"#),
    ]);
    let initial = context(1.0, 2.0, 3.0, 4.0, 5.0);

    assert_eq!(
        vm.execute_function("example:all_transforms", None, initial, 6),
        Err(ExecutionError::CommandLimitExceeded { limit: 6 })
    );
    assert_eq!(
        vm.execute_function("example:all_transforms", None, initial, 7),
        Ok(returned(61))
    );
    assert_eq!(
        vm.execute_function("example:inactive", None, initial, 4),
        Err(ExecutionError::CommandLimitExceeded { limit: 4 })
    );
    assert_eq!(
        vm.execute_function("example:inactive", None, initial, 5),
        Ok(ExecutionOutcome::Result {
            success: false,
            value: 0,
        })
    );
}
