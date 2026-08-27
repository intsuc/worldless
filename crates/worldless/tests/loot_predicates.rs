mod common;

use common::context;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{
    FunctionOutcome, LoadError, MemoryResource, Pack, ResourceKind, ResourceOrigin, Vm,
};

const LIMIT: usize = 256;
static NEXT_PACK: AtomicU64 = AtomicU64::new(0);

struct TestPack {
    root: PathBuf,
}

impl TestPack {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-loot-predicate-test-{}-{}",
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

fn resources(
    functions: &[(&str, &str)],
    number_providers: &[(&str, &str)],
    predicates: &[(&str, &str)],
    predicate_tags: &[(&str, &str)],
) -> Result<Vm, LoadError> {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let number_providers = number_providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::NumberProvider, *id, *source));
    let predicates = predicates
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Predicate, *id, *source));
    let predicate_tags = predicate_tags
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::PredicateTag, *id, *source));
    Vm::from_packs([Pack::memory(
        functions
            .chain(number_providers)
            .chain(predicates)
            .chain(predicate_tags),
    )])
}

fn compile(
    functions: &[(&str, &str)],
    number_providers: &[(&str, &str)],
    predicates: &[(&str, &str)],
    predicate_tags: &[(&str, &str)],
) -> Vm {
    resources(functions, number_providers, predicates, predicate_tags).unwrap()
}

fn returned(success: bool, value: i32) -> FunctionOutcome {
    FunctionOutcome::Returned { success, value }
}

fn assert_function(vm: &mut Vm, function: &str, expected: FunctionOutcome) {
    assert_eq!(
        vm.execute_function(function, context(), LIMIT).unwrap(),
        expected,
        "{function}"
    );
}

#[test]
fn composites_and_value_checks_follow_minecraft_semantics() {
    let predicates = [
        ("example:truth", r#"{"type":"all_of","terms":[]}"#),
        ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        (
            "example:not_false",
            r#"{"type":"inverted","term":"example:falsehood"}"#,
        ),
        (
            "example:all",
            r#"{"type":"all_of","terms":["example:truth",{"type":"inverted","term":"example:falsehood"}]}"#,
        ),
        (
            "example:any",
            r#"{"type":"any_of","terms":["example:falsehood","example:truth"]}"#,
        ),
        (
            "example:compact_terms",
            r#"{"type":"all_of","terms":"example:truth"}"#,
        ),
        (
            "example:rounded_positive",
            r#"{"type":"value_check","value":1.5,"range":2}"#,
        ),
        (
            "example:rounded_negative",
            r#"{"type":"value_check","value":-1.5,"range":-1}"#,
        ),
        (
            "example:minimum",
            r#"{"type":"value_check","value":3,"range":{"min":3}}"#,
        ),
        (
            "example:maximum",
            r#"{"type":"value_check","value":3,"range":{"max":3}}"#,
        ),
        (
            "example:closed",
            r#"{"type":"value_check","value":3,"range":{"min":3,"max":3}}"#,
        ),
        (
            "example:unbounded",
            r#"{"type":"value_check","value":123,"range":{}}"#,
        ),
        (
            "example:outside",
            r#"{"type":"value_check","value":3,"range":{"min":4,"max":2}}"#,
        ),
    ];
    let functions = [
        (
            "example:truth",
            "return run execute if predicate example:truth\n",
        ),
        (
            "example:falsehood",
            "return run execute if predicate example:falsehood\n",
        ),
        (
            "example:not_false",
            "return run execute if predicate example:not_false\n",
        ),
        (
            "example:all",
            "return run execute if predicate example:all\n",
        ),
        (
            "example:any",
            "return run execute if predicate example:any\n",
        ),
        (
            "example:compact_terms",
            "return run execute if predicate example:compact_terms\n",
        ),
        (
            "example:rounded_positive",
            "return run execute if predicate example:rounded_positive\n",
        ),
        (
            "example:rounded_negative",
            "return run execute if predicate example:rounded_negative\n",
        ),
        (
            "example:minimum",
            "return run execute if predicate example:minimum\n",
        ),
        (
            "example:maximum",
            "return run execute if predicate example:maximum\n",
        ),
        (
            "example:closed",
            "return run execute if predicate example:closed\n",
        ),
        (
            "example:unbounded",
            "return run execute if predicate example:unbounded\n",
        ),
        (
            "example:outside",
            "return run execute unless predicate example:outside\n",
        ),
    ];
    let mut vm = compile(&functions, &[], &predicates, &[]);

    for function in [
        "example:truth",
        "example:not_false",
        "example:all",
        "example:any",
        "example:compact_terms",
        "example:rounded_positive",
        "example:rounded_negative",
        "example:minimum",
        "example:maximum",
        "example:closed",
        "example:unbounded",
        "example:outside",
    ] {
        assert_function(&mut vm, function, returned(true, 1));
    }
    assert_function(&mut vm, "example:falsehood", returned(false, 0));
}

#[test]
fn resources_tags_references_and_inline_predicates_resolve() {
    let mut vm = compile(
        &[
            (
                "example:tagged",
                "return run execute if predicate example:tagged run return 5\n",
            ),
            (
                "example:inline_references",
                r#"return run execute if predicate {type:"all_of",terms:["example:truth","example:not_false"]} run return 6
"#,
            ),
            (
                "example:inline_predicate",
                r#"return run execute if predicate {type:"value_check",value:1.5,range:2} run return 7
"#,
            ),
        ],
        &[],
        &[
            ("example:truth", r#"{"type":"all_of","terms":[]}"#),
            ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
            (
                "example:not_false",
                r#"{"type":"inverted","term":"example:falsehood"}"#,
            ),
            (
                "example:tagged",
                r##"{"type":"any_of","terms":"#example:outer"}"##,
            ),
        ],
        &[
            (
                "example:outer",
                r##"{"values":["example:falsehood",{"id":"example:missing","required":false},"#example:tail"]}"##,
            ),
            ("example:tail", r#"{"values":["example:truth"]}"#),
        ],
    );

    assert_function(&mut vm, "example:tagged", returned(true, 5));
    assert_function(&mut vm, "example:inline_references", returned(true, 6));
    assert_function(&mut vm, "example:inline_predicate", returned(true, 7));
}

#[test]
fn directory_loader_reads_predicate_resources_and_tags() {
    let pack = TestPack::new();
    pack.write(
        "data/example/function/main.mcfunction",
        "return run execute if predicate example:tagged run return 7\n",
    );
    pack.write(
        "data/example/predicate/truth.json",
        r#"{"type":"all_of","terms":[]}"#,
    );
    pack.write(
        "data/example/predicate/falsehood.json",
        r#"{"type":"any_of","terms":[]}"#,
    );
    pack.write(
        "data/example/predicate/tagged.json",
        r##"{"type":"any_of","terms":"#example:checks"}"##,
    );
    pack.write(
        "data/example/tags/predicate/checks.json",
        r#"{"values":["example:falsehood","example:truth"]}"#,
    );

    let mut vm = Vm::from_packs([Pack::directory(pack.root())]).unwrap();
    assert_function(&mut vm, "example:main", returned(true, 7));
}

#[test]
fn directory_loader_reports_the_invalid_predicate_path_and_reason() {
    let pack = TestPack::new();
    let relative_path = "data/example/predicate/invalid.json";
    pack.write(relative_path, r#"{"type":"all_of"}"#);
    let expected_path = pack.root().join(relative_path);

    match Vm::from_packs([Pack::directory(pack.root())]).unwrap_err() {
        LoadError::InvalidPredicate { origin, reason } => {
            assert_eq!(origin, ResourceOrigin::Directory(expected_path));
            assert_eq!(reason, "`root` is missing field `terms`");
        }
        error => panic!("expected an invalid predicate error, got {error}"),
    }
}

#[test]
fn vanilla_world_dependent_predicates_exist_with_their_worldless_result() {
    let mut vm = compile(
        &[
            (
                "example:silk_touch",
                "return run execute unless predicate minecraft:tool/can_silk_touch\n",
            ),
            (
                "example:tagged_shears",
                "return run execute unless predicate example:tagged_shears\n",
            ),
        ],
        &[],
        &[(
            "example:tagged_shears",
            r##"{"type":"all_of","terms":"#example:shears"}"##,
        )],
        &[(
            "example:shears",
            r#"{"values":[{"id":"minecraft:tool/can_shear","required":false}]}"#,
        )],
    );

    assert_function(&mut vm, "example:silk_touch", returned(true, 1));
    assert_function(&mut vm, "example:tagged_shears", returned(true, 1));
}

#[test]
fn macro_instantiation_uses_the_same_predicate_registry_and_inline_parser() {
    let mut vm = compile(
        &[
            (
                "example:named_macro",
                "$return run execute if predicate example:$(predicate)\n",
            ),
            (
                "example:inline_macro",
                "$return run execute if predicate {type:$(kind),terms:[]}\n",
            ),
            (
                "example:named_call",
                "return run function example:named_macro {predicate:\"truth\"}\n",
            ),
            (
                "example:inline_call",
                "return run function example:inline_macro {kind:\"all_of\"}\n",
            ),
        ],
        &[],
        &[("example:truth", r#"{"type":"all_of","terms":[]}"#)],
        &[],
    );

    assert_function(&mut vm, "example:named_call", returned(true, 1));
    assert_function(&mut vm, "example:inline_call", returned(true, 1));
}

fn uniform_after(predicate: &str, predicate_tags: &[(&str, &str)]) -> i32 {
    let mut vm = compile(
        &[(
            "example:main",
            "execute if predicate example:test\nreturn run compute default {type:uniform,min:0,max:10}\n",
        )],
        &[],
        &[
            ("example:truth", r#"{"type":"all_of","terms":[]}"#),
            ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
            ("example:random", r#"{"type":"random_chance","chance":1}"#),
            ("example:test", predicate),
        ],
        predicate_tags,
    );
    match vm
        .execute_function("example:main", context(), LIMIT)
        .unwrap()
    {
        FunctionOutcome::Returned {
            success: true,
            value,
        } => value,
        outcome => panic!("expected a successful returned uniform value, got {outcome:?}"),
    }
}

#[test]
fn composites_and_tags_short_circuit_in_declared_order() {
    assert_eq!(
        uniform_after(
            r#"{"type":"all_of","terms":["example:falsehood","example:random"]}"#,
            &[],
        ),
        7
    );
    assert_eq!(
        uniform_after(
            r#"{"type":"any_of","terms":["example:truth","example:random"]}"#,
            &[],
        ),
        7
    );
    assert_eq!(
        uniform_after(
            r#"{"type":"all_of","terms":["example:random","example:falsehood"]}"#,
            &[],
        ),
        8
    );
    assert_eq!(
        uniform_after(
            r##"{"type":"any_of","terms":"#example:ordered"}"##,
            &[(
                "example:ordered",
                r#"{"values":["example:random","example:truth"]}"#,
            )],
        ),
        8
    );
}

#[test]
fn random_chance_evaluates_its_provider_before_sampling() {
    let mut vm = compile(
        &[
            (
                "example:test",
                "return run execute if predicate example:random\n",
            ),
            (
                "example:next",
                "return run compute default {type:uniform,min:0,max:10}\n",
            ),
        ],
        &[],
        &[(
            "example:random",
            r#"{"type":"random_chance","chance":{"type":"uniform","min":0,"max":1}}"#,
        )],
        &[],
    );

    assert_function(&mut vm, "example:test", returned(false, 0));
    assert_function(&mut vm, "example:next", returned(true, 2));

    let mut repeated = compile(
        &[(
            "example:test",
            "return run execute if predicate example:random\n",
        )],
        &[("example:chance", "0.75")],
        &[(
            "example:random",
            r#"{"type":"random_chance","chance":"example:chance"}"#,
        )],
        &[],
    );
    for expected in [true, false, true] {
        assert_function(
            &mut repeated,
            "example:test",
            returned(expected, i32::from(expected)),
        );
    }
}

#[test]
fn predicate_conditions_publish_terminal_results_and_preserve_modifier_order() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #stored state 4\n",
            ),
            (
                "example:terminal_if",
                "return run execute if predicate example:truth\n",
            ),
            (
                "example:terminal_unless",
                "return run execute unless predicate example:falsehood\n",
            ),
            (
                "example:terminal_failure",
                "return run execute if predicate example:falsehood\n",
            ),
            (
                "example:redirect",
                "return run execute if predicate example:truth run return 9\n",
            ),
            (
                "example:filtered_return",
                "return run execute if predicate example:falsehood run return 9\nreturn 12\n",
            ),
            (
                "example:store_before_false",
                "execute store result score #stored state if predicate example:falsehood run return 9\nreturn run scoreboard players get #stored state\n",
            ),
            (
                "example:terminal_result_store",
                "execute store result score #terminal state if predicate example:truth\nreturn run scoreboard players get #terminal state\n",
            ),
            (
                "example:terminal_success_store",
                "scoreboard players set #terminal state 9\nexecute store success score #terminal state if predicate example:falsehood\nreturn run scoreboard players get #terminal state\n",
            ),
            (
                "example:store_after_true",
                "execute if predicate example:truth store result score #downstream state run scoreboard players set #source state 9\nreturn run scoreboard players get #downstream state\n",
            ),
        ],
        &[],
        &[
            ("example:truth", r#"{"type":"all_of","terms":[]}"#),
            ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        ],
        &[],
    );

    assert_function(&mut vm, "example:setup", FunctionOutcome::FellThrough);
    assert_function(&mut vm, "example:terminal_if", returned(true, 1));
    assert_function(&mut vm, "example:terminal_unless", returned(true, 1));
    assert_function(&mut vm, "example:terminal_failure", returned(false, 0));
    assert_function(&mut vm, "example:redirect", returned(true, 9));
    assert_function(&mut vm, "example:filtered_return", returned(false, 0));
    assert_function(&mut vm, "example:store_before_false", returned(true, 4));
    assert_function(&mut vm, "example:terminal_result_store", returned(true, 1));
    assert_function(&mut vm, "example:terminal_success_store", returned(true, 0));
    assert_function(&mut vm, "example:store_after_true", returned(true, 9));
}

#[test]
fn conditional_and_dispatcher_select_their_minecraft_branches() {
    let mut vm = compile(
        &[
            (
                "example:conditional_true",
                "return run compute default example:conditional_true integer\n",
            ),
            (
                "example:conditional_false",
                "return run compute default example:conditional_false integer\n",
            ),
            (
                "example:conditional_default",
                "return run compute default example:conditional_default integer\n",
            ),
            (
                "example:inline_conditional",
                "return run compute default {type:conditional,condition:{type:all_of,terms:[]},on_true:6} integer\n",
            ),
            (
                "example:dispatcher",
                "return run compute default example:dispatcher integer\n",
            ),
            (
                "example:dispatcher_default",
                "return run compute default example:dispatcher_default integer\n",
            ),
            (
                "example:dispatcher_implicit",
                "return run compute default example:dispatcher_implicit integer\n",
            ),
        ],
        &[
            (
                "example:conditional_true",
                r#"{"type":"conditional","condition":"example:truth","on_true":7,"on_false":9}"#,
            ),
            (
                "example:conditional_false",
                r#"{"type":"conditional","condition":"example:falsehood","on_true":7,"on_false":9}"#,
            ),
            (
                "example:conditional_default",
                r#"{"type":"conditional","condition":"example:falsehood","on_true":7}"#,
            ),
            (
                "example:dispatcher",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","number_provider":100},{"condition":"example:truth","number_provider":4},{"condition":"example:truth","number_provider":9}],"default":11}"#,
            ),
            (
                "example:dispatcher_default",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","number_provider":100}],"default":11}"#,
            ),
            (
                "example:dispatcher_implicit",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","number_provider":100}]}"#,
            ),
        ],
        &[
            ("example:truth", r#"{"type":"all_of","terms":[]}"#),
            ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        ],
        &[],
    );

    for (function, value) in [
        ("example:conditional_true", 7),
        ("example:conditional_false", 9),
        ("example:conditional_default", 0),
        ("example:inline_conditional", 6),
        ("example:dispatcher", 4),
        ("example:dispatcher_default", 11),
        ("example:dispatcher_implicit", 0),
    ] {
        assert_function(&mut vm, function, returned(true, value));
    }
}

#[test]
fn conditional_and_dispatcher_do_not_evaluate_unselected_values_or_later_cases() {
    let predicates = [
        ("example:truth", r#"{"type":"all_of","terms":[]}"#),
        ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        ("example:random", r#"{"type":"random_chance","chance":1}"#),
    ];
    let mut conditional = compile(
        &[
            (
                "example:conditional",
                "return run compute default example:conditional integer\n",
            ),
            (
                "example:next",
                "return run compute default example:uniform\n",
            ),
        ],
        &[
            ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
            (
                "example:conditional",
                r#"{"type":"conditional","condition":"example:falsehood","on_true":"example:uniform","on_false":4}"#,
            ),
        ],
        &predicates,
        &[],
    );
    assert_function(&mut conditional, "example:conditional", returned(true, 4));
    assert_function(&mut conditional, "example:next", returned(true, 7));

    let mut dispatcher = compile(
        &[
            (
                "example:dispatcher",
                "return run compute default example:dispatcher integer\n",
            ),
            (
                "example:next",
                "return run compute default example:uniform\n",
            ),
        ],
        &[
            ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
            (
                "example:dispatcher",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","number_provider":"example:uniform"},{"condition":"example:truth","number_provider":4},{"condition":"example:random","number_provider":5}],"default":6}"#,
            ),
        ],
        &predicates,
        &[],
    );
    assert_function(&mut dispatcher, "example:dispatcher", returned(true, 4));
    assert_function(&mut dispatcher, "example:next", returned(true, 7));
}

#[test]
fn invalid_predicates_references_and_cross_registry_cycles_are_rejected() {
    for (id, source, expected) in [
        ("example:json", "not json", "invalid JSON"),
        (
            "example:missing_terms",
            r#"{"type":"all_of"}"#,
            "missing field `terms`",
        ),
        (
            "example:unsupported",
            r#"{"type":"match_block"}"#,
            "outside Worldless scope",
        ),
        (
            "example:unreachable_unsupported",
            r#"{"type":"all_of","terms":[{"type":"any_of","terms":[]},{"type":"match_block"}]}"#,
            "outside Worldless scope",
        ),
        (
            "example:missing_predicate",
            r#"{"type":"inverted","term":"example:missing"}"#,
            "does not exist",
        ),
        (
            "example:missing_provider",
            r#"{"type":"random_chance","chance":"example:missing"}"#,
            "does not exist",
        ),
        (
            "example:missing_tag",
            r##"{"type":"all_of","terms":"#example:missing"}"##,
            "does not exist",
        ),
    ] {
        let error = resources(&[], &[], &[(id, source)], &[]).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }

    let missing_condition = resources(
        &[],
        &[(
            "example:conditional",
            r#"{"type":"conditional","condition":"example:missing","on_true":1}"#,
        )],
        &[],
        &[],
    )
    .unwrap_err();
    assert!(
        missing_condition.to_string().contains("does not exist"),
        "{missing_condition}"
    );

    let predicate_cycle = resources(
        &[],
        &[],
        &[
            (
                "example:first",
                r#"{"type":"inverted","term":"example:second"}"#,
            ),
            (
                "example:second",
                r#"{"type":"inverted","term":"example:first"}"#,
            ),
        ],
        &[],
    )
    .unwrap_err();
    assert!(
        predicate_cycle.to_string().contains("cyclic"),
        "{predicate_cycle}"
    );

    let resource_tag_cycle = resources(
        &[],
        &[],
        &[(
            "example:predicate",
            r##"{"type":"all_of","terms":"#example:self"}"##,
        )],
        &[("example:self", r#"{"values":["example:predicate"]}"#)],
    )
    .unwrap_err();
    assert!(
        resource_tag_cycle.to_string().contains("cyclic"),
        "{resource_tag_cycle}"
    );

    let tag_cycle = resources(
        &[],
        &[],
        &[],
        &[
            ("example:first", r##"{"values":["#example:second"]}"##),
            ("example:second", r##"{"values":["#example:first"]}"##),
        ],
    )
    .unwrap_err();
    assert!(tag_cycle.to_string().contains("cyclic"), "{tag_cycle}");

    let cross_registry_cycle = resources(
        &[],
        &[(
            "example:provider",
            r#"{"type":"conditional","condition":"example:predicate","on_true":1}"#,
        )],
        &[(
            "example:predicate",
            r#"{"type":"value_check","value":"example:provider","range":1}"#,
        )],
        &[],
    )
    .unwrap_err();
    assert!(
        cross_registry_cycle.to_string().contains("cyclic"),
        "{cross_registry_cycle}"
    );

    let random_chance_cycle = resources(
        &[],
        &[(
            "example:provider",
            r#"{"type":"conditional","condition":"example:predicate","on_true":1}"#,
        )],
        &[(
            "example:predicate",
            r#"{"type":"random_chance","chance":"example:provider"}"#,
        )],
        &[],
    )
    .unwrap_err();
    assert!(
        random_chance_cycle.to_string().contains("cyclic"),
        "{random_chance_cycle}"
    );

    for command in [
        "return run execute if predicate example:missing",
        "return run execute if predicate #example:predicates",
    ] {
        let error = resources(
            &[("example:invalid", command)],
            &[],
            &[("example:truth", r#"{"type":"all_of","terms":[]}"#)],
            &[("example:predicates", r#"{"values":["example:truth"]}"#)],
        )
        .unwrap_err();
        assert!(matches!(error, LoadError::InvalidFunction { .. }));
    }
}
