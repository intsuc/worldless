mod common;

use common::context;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{
    ExecutionOutcome, LoadError, MemoryResource, Pack, ResourceKind, ResourceOrigin, Vm,
};

const LIMIT: usize = 256;
static NEXT_PACK: AtomicU64 = AtomicU64::new(0);

struct TestPack {
    root: PathBuf,
}

impl TestPack {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-pack-composition-test-{}-{}",
            std::process::id(),
            NEXT_PACK.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("pack.mcmeta"),
            r#"{"pack":{"description":"test","min_format":[118,0],"max_format":[118,0]}}"#,
        )
        .unwrap();
        Self { root }
    }

    fn write(&self, relative_path: &str, contents: &str) {
        let path = self.root.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TestPack {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!(
                "failed to remove test pack {}: {error}",
                self.root.display()
            );
        }
    }
}

fn resource(kind: ResourceKind, id: &str, source: &str) -> MemoryResource {
    MemoryResource::new(kind, id, source)
}

fn function(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::Function, id, source)
}

fn function_tag(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::FunctionTag, id, source)
}

fn number_provider(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::NumberProvider, id, source)
}

fn number_provider_tag(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::NumberProviderTag, id, source)
}

fn predicate(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::Predicate, id, source)
}

fn predicate_tag(id: &str, source: &str) -> MemoryResource {
    resource(ResourceKind::PredicateTag, id, source)
}

fn returned(value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result {
        success: true,
        value,
    }
}

fn assert_function(vm: &mut Vm, id: &str, expected: ExecutionOutcome) {
    assert_eq!(
        vm.execute_function(id, None, context(), LIMIT).unwrap(),
        expected,
        "{id}"
    );
}

#[test]
fn later_packs_override_ordinary_resources_without_hiding_unrelated_resources() {
    let low = Pack::memory([
        function("example:function_winner", "return 1\n"),
        function("example:low_only", "return 4\n"),
        function(
            "example:provider_result",
            "return run compute default example:number\n",
        ),
        function(
            "example:predicate_result",
            "return run execute if predicate example:condition run return 9\n",
        ),
        number_provider("example:number", "1"),
        predicate("example:condition", r#"{"type":"any_of","terms":[]}"#),
    ]);
    let high = Pack::memory([
        function("example:function_winner", "return 2\n"),
        function("example:high_only", "return 5\n"),
        number_provider("example:number", "7"),
        predicate("example:condition", r#"{"type":"all_of","terms":[]}"#),
    ]);

    let mut vm = Vm::from_packs([low, high], 0).unwrap();
    for (id, value) in [
        ("example:function_winner", 2),
        ("example:low_only", 4),
        ("example:high_only", 5),
        ("example:provider_result", 7),
        ("example:predicate_result", 9),
    ] {
        assert_function(&mut vm, id, returned(value));
    }
}

#[test]
fn only_the_selected_ordinary_resource_is_validated() {
    let low = Pack::memory([
        function("example:function_winner", "not a command\n"),
        number_provider("example:number", "{"),
        predicate("example:condition", "{"),
    ]);
    let high = Pack::memory([
        function("example:function_winner", "return 8\n"),
        function(
            "example:provider_result",
            "return run compute default example:number\n",
        ),
        function(
            "example:predicate_result",
            "return run execute if predicate example:condition run return 9\n",
        ),
        number_provider("example:number", "6"),
        predicate("example:condition", r#"{"type":"all_of","terms":[]}"#),
    ]);

    let mut vm = Vm::from_packs([low, high], 0).unwrap();
    assert_function(&mut vm, "example:function_winner", returned(8));
    assert_function(&mut vm, "example:provider_result", returned(6));
    assert_function(&mut vm, "example:predicate_result", returned(9));

    let error = Vm::from_packs(
        [
            Pack::memory([function("example:broken", "return 1\n")]),
            Pack::memory([function("example:broken", "not a command\n")]),
        ],
        0,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::InvalidFunction {
            origin: ResourceOrigin::Memory { pack: 1, ref id },
            line: 1,
            ..
        } if id == "example:broken"
    ));

    let directory = TestPack::new();
    directory.write("data/example/function/broken.mcfunction", "not a command\n");
    let expected_path = directory
        .root()
        .join("data/example/function/broken.mcfunction");
    let error = Vm::from_packs(
        [
            Pack::memory([function("example:broken", "return 1\n")]),
            Pack::directory(directory.root()),
        ],
        0,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::InvalidFunction {
            origin: ResourceOrigin::Directory(path),
            line: 1,
            ..
        } if path == expected_path
    ));
}

#[test]
fn function_tags_append_replace_and_resolve_after_all_packs_are_composed() {
    let low = Pack::memory([
        function(
            "example:setup",
            "scoreboard objectives add state dummy\nscoreboard players set #trace state 0\nscoreboard players set #ten state 10\n",
        ),
        function("example:reset", "scoreboard players set #trace state 0\n"),
        function(
            "example:one",
            "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 1\n",
        ),
        function(
            "example:two",
            "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 2\n",
        ),
        function(
            "example:three",
            "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 3\n",
        ),
        function(
            "example:four",
            "scoreboard players operation #trace state *= #ten state\nscoreboard players add #trace state 4\n",
        ),
        function(
            "example:run_append",
            "function #example:append\nreturn run scoreboard players get #trace state\n",
        ),
        function(
            "example:run_replaced",
            "function #example:replaced\nreturn run scoreboard players get #trace state\n",
        ),
        function(
            "example:run_old_cycle",
            "function #example:old_cycle\nreturn run scoreboard players get #trace state\n",
        ),
        function_tag("example:append", r#"{"values":["example:one"]}"#),
        function_tag("example:tail", r#"{"values":["example:two"]}"#),
        function_tag(
            "example:replaced",
            r##"{"values":["example:missing","#example:old_cycle","example:one"]}"##,
        ),
        function_tag("example:old_cycle", r##"{"values":["#example:replaced"]}"##),
    ]);
    let high = Pack::memory([
        function_tag(
            "example:append",
            r##"{"values":["#example:tail","example:three","example:one","example:two"]}"##,
        ),
        function_tag(
            "example:replaced",
            r#"{"replace":true,"values":["example:four"]}"#,
        ),
    ]);

    let mut vm = Vm::from_packs([low, high], 0).unwrap();
    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:run_append", returned(123));
    assert_function(&mut vm, "example:reset", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:run_replaced", returned(4));
    assert_function(&mut vm, "example:reset", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:run_old_cycle", returned(4));
}

#[test]
fn registry_resource_tags_are_composed_before_their_consumers_are_resolved() {
    let low = Pack::memory([
        function(
            "example:provider_result",
            "return run compute default example:sum\n",
        ),
        function(
            "example:predicate_result",
            "return run execute if predicate example:tagged run return 7\n",
        ),
        number_provider("example:one", "1"),
        number_provider("example:two", "2"),
        number_provider(
            "example:sum",
            r##"{"type":"sum","operands":"#example:values"}"##,
        ),
        number_provider_tag("example:values", r#"{"values":["example:one"]}"#),
        predicate("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        predicate("example:truth", r#"{"type":"all_of","terms":[]}"#),
        predicate(
            "example:tagged",
            r##"{"type":"any_of","terms":"#example:checks"}"##,
        ),
        predicate_tag("example:checks", r#"{"values":["example:falsehood"]}"#),
    ]);
    let high = Pack::memory([
        number_provider_tag(
            "example:values",
            r#"{"values":["example:two","example:one"]}"#,
        ),
        predicate_tag(
            "example:checks",
            r#"{"values":["example:truth","example:falsehood"]}"#,
        ),
    ]);

    let mut vm = Vm::from_packs([low, high], 0).unwrap();
    assert_function(&mut vm, "example:provider_result", returned(3));
    assert_function(&mut vm, "example:predicate_result", returned(7));
}

#[test]
fn duplicate_normalized_ids_are_rejected_within_one_pack_but_legal_across_packs() {
    let error = Vm::from_packs(
        [Pack::memory([
            function("value", "return 1\n"),
            function("minecraft:value", "return 2\n"),
        ])],
        0,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::DuplicateMemoryResource {
            pack: 0,
            kind: ResourceKind::Function,
            ref id,
        } if id == "minecraft:value"
    ));

    let mut vm = Vm::from_packs(
        [
            Pack::memory([function("value", "return 1\n")]),
            Pack::memory([function("minecraft:value", "return 2\n")]),
        ],
        0,
    )
    .unwrap();
    assert_function(&mut vm, "minecraft:value", returned(2));
}

#[test]
fn directory_and_memory_packs_share_the_same_priority_order() {
    let directory = TestPack::new();
    directory.write("data/example/function/winner.mcfunction", "return 1\n");
    directory.write(
        "data/example/function/directory_only.mcfunction",
        "return 3\n",
    );

    let mut memory_wins = Vm::from_packs(
        [
            Pack::directory(directory.root()),
            Pack::memory([function("example:winner", "return 2\n")]),
        ],
        0,
    )
    .unwrap();
    assert_function(&mut memory_wins, "example:winner", returned(2));
    assert_function(&mut memory_wins, "example:directory_only", returned(3));

    let mut directory_wins = Vm::from_packs(
        [
            Pack::memory([function("example:winner", "return 2\n")]),
            Pack::directory(directory.root()),
        ],
        0,
    )
    .unwrap();
    assert_function(&mut directory_wins, "example:winner", returned(1));
}

#[test]
fn an_empty_pack_stack_builds_an_empty_vm() {
    let mut vm = Vm::from_packs(std::iter::empty::<Pack>(), 0).unwrap();
    assert_eq!(
        vm.execute_function("example:missing", None, context(), LIMIT)
            .unwrap(),
        ExecutionOutcome::Result {
            success: false,
            value: 0,
        }
    );
}

#[test]
fn filters_and_overlays_remain_explicitly_unsupported() {
    for (section, feature) in [
        (r#""filter":{"block":[]}"#, "pack filters"),
        (r#""overlays":{"entries":[]}"#, "resource overlays"),
    ] {
        let directory = TestPack::new();
        fs::write(
            directory.root().join("pack.mcmeta"),
            format!(
                r#"{{"pack":{{"description":"test","min_format":[118,0],"max_format":[118,0]}},{section}}}"#
            ),
        )
        .unwrap();

        let error = Vm::from_packs([Pack::directory(directory.root())], 0).unwrap_err();
        assert!(matches!(
            error,
            LoadError::UnsupportedPack {
                feature: actual,
                ..
            } if actual == feature
        ));
    }
}
