mod common;

use common::context;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{
    CompiledProgram, ExecutionError, ExecutionOutcome, LoadError, MemoryResource, Pack,
    ResourceKind, ResourceOrigin, Vm,
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
            r#"{"pack":{"description":"test","min_format":[119,0],"max_format":[119,0]}}"#,
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
    int_providers: &[(&str, &str)],
    float_providers: &[(&str, &str)],
    predicates: &[(&str, &str)],
    predicate_tags: &[(&str, &str)],
) -> Result<Vm, LoadError> {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let int_providers = int_providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::ContextIntProvider, *id, *source));
    let float_providers = float_providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::ContextFloatProvider, *id, *source));
    let predicates = predicates
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Predicate, *id, *source));
    let predicate_tags = predicate_tags
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::PredicateTag, *id, *source));
    CompiledProgram::from_packs([Pack::memory(
        functions
            .chain(int_providers)
            .chain(float_providers)
            .chain(predicates)
            .chain(predicate_tags),
    )])
    .map(|program| program.create_vm(0))
}

fn compile(
    functions: &[(&str, &str)],
    int_providers: &[(&str, &str)],
    float_providers: &[(&str, &str)],
    predicates: &[(&str, &str)],
    predicate_tags: &[(&str, &str)],
) -> Vm {
    resources(
        functions,
        int_providers,
        float_providers,
        predicates,
        predicate_tags,
    )
    .unwrap()
}

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn assert_function(vm: &mut Vm, function: &str, expected: ExecutionOutcome) {
    assert_eq!(
        vm.execute_function(function, None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        expected,
        "{function}"
    );
}

#[test]
fn composites_and_typed_value_checks_follow_minecraft_semantics() {
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
            "example:float_positive",
            r#"{"type":"float_value_check","value":1.5,"test":1.5}"#,
        ),
        (
            "example:float_negative",
            r#"{"type":"float_value_check","value":-1.5,"test":-1.5}"#,
        ),
        (
            "example:minimum",
            r#"{"type":"int_value_check","value":3,"test":{"min":3}}"#,
        ),
        (
            "example:maximum",
            r#"{"type":"int_value_check","value":3,"test":{"max":3}}"#,
        ),
        (
            "example:closed",
            r#"{"type":"int_value_check","value":3,"test":{"min":3,"max":3}}"#,
        ),
        (
            "example:unbounded",
            r#"{"type":"int_value_check","value":123,"test":{}}"#,
        ),
        (
            "example:outside",
            r#"{"type":"int_value_check","value":3,"test":{"min":4,"max":2}}"#,
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
            "example:float_positive",
            "return run execute if predicate example:float_positive\n",
        ),
        (
            "example:float_negative",
            "return run execute if predicate example:float_negative\n",
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
    let mut vm = compile(&functions, &[], &[], &predicates, &[]);

    for function in [
        "example:truth",
        "example:not_false",
        "example:all",
        "example:any",
        "example:compact_terms",
        "example:float_positive",
        "example:float_negative",
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
                r#"return run execute if predicate {type:"float_value_check",value:1.5,test:1.5} run return 7
"#,
            ),
        ],
        &[],
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

    let mut vm = CompiledProgram::from_packs([Pack::directory(pack.root())])
        .map(|program| program.create_vm(0))
        .unwrap();
    assert_function(&mut vm, "example:main", returned(true, 7));
}

#[test]
fn directory_loader_reports_the_invalid_predicate_path_and_reason() {
    let pack = TestPack::new();
    let relative_path = "data/example/predicate/invalid.json";
    pack.write(relative_path, r#"{"type":"all_of"}"#);
    let expected_path = pack.root().join(relative_path);

    match CompiledProgram::from_packs([Pack::directory(pack.root())])
        .map(|program| program.create_vm(0))
        .unwrap_err()
    {
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
fn absent_command_context_parameters_produce_their_minecraft_results() {
    let mut vm = compile(
        &[
            (
                "example:survives",
                "return run execute if predicate example:survives\n",
            ),
            (
                "example:killed",
                "return run execute if predicate example:killed\n",
            ),
            (
                "example:entity_without_predicate",
                "return run execute if predicate example:entity_without_predicate\n",
            ),
            (
                "example:entity_with_empty_predicate",
                "return run execute if predicate example:entity_with_empty_predicate\n",
            ),
            (
                "example:scores",
                "return run execute if predicate example:scores\n",
            ),
            (
                "example:block",
                "return run execute if predicate example:block\n",
            ),
            (
                "example:tool_without_predicate",
                "return run execute if predicate example:tool_without_predicate\n",
            ),
            (
                "example:tool_with_empty_predicate",
                "return run execute if predicate example:tool_with_empty_predicate\n",
            ),
            (
                "example:damage_without_predicate",
                "return run execute if predicate example:damage_without_predicate\n",
            ),
            (
                "example:damage_with_empty_predicate",
                "return run execute if predicate example:damage_with_empty_predicate\n",
            ),
            (
                "example:weather",
                "return run execute if predicate example:weather\n",
            ),
        ],
        &[("example:bound", "1")],
        &[],
        &[
            ("example:survives", r#"{"type":"survives_explosion"}"#),
            ("example:killed", r#"{"type":"killed_by_player"}"#),
            (
                "example:entity_without_predicate",
                r#"{"type":"entity_properties","entity":"this"}"#,
            ),
            (
                "example:entity_with_empty_predicate",
                r#"{"type":"entity_properties","entity":"this","predicate":{}}"#,
            ),
            (
                "example:scores",
                r#"{"type":"entity_scores","scores":{"points":{"min":"example:bound"}},"entity":"attacker"}"#,
            ),
            ("example:block", r#"{"type":"match_block"}"#),
            ("example:tool_without_predicate", r#"{"type":"match_tool"}"#),
            (
                "example:tool_with_empty_predicate",
                r#"{"type":"match_tool","predicate":{}}"#,
            ),
            (
                "example:damage_without_predicate",
                r#"{"type":"damage_source_properties"}"#,
            ),
            (
                "example:damage_with_empty_predicate",
                r#"{"type":"damage_source_properties","predicate":{}}"#,
            ),
            ("example:weather", r#"{"type":"weather_check"}"#),
        ],
        &[],
    );

    for function in [
        "example:survives",
        "example:entity_without_predicate",
        "example:weather",
    ] {
        assert_function(&mut vm, function, returned(true, 1));
    }
    for function in [
        "example:killed",
        "example:entity_with_empty_predicate",
        "example:scores",
        "example:block",
        "example:tool_without_predicate",
        "example:tool_with_empty_predicate",
        "example:damage_without_predicate",
        "example:damage_with_empty_predicate",
    ] {
        assert_function(&mut vm, function, returned(false, 0));
    }
}

#[test]
fn absent_context_results_do_not_consume_randomness_or_evaluate_score_ranges() {
    for predicate in [
        r#"{"type":"survives_explosion"}"#,
        r#"{"type":"killed_by_player"}"#,
        r#"{"type":"entity_properties","entity":"this"}"#,
        r#"{"type":"entity_properties","entity":"this","predicate":{}}"#,
        r#"{"type":"entity_scores","scores":{"points":{"min":{"type":"uniform","min":0,"max":1}}},"entity":"this"}"#,
        r#"{"type":"match_block"}"#,
        r#"{"type":"match_tool"}"#,
        r#"{"type":"damage_source_properties"}"#,
        r#"{"type":"weather_check"}"#,
    ] {
        assert_eq!(uniform_after(predicate, &[]), 7, "{predicate}");
    }
}

#[test]
fn missing_required_context_is_an_evaluation_error_and_respects_short_circuiting() {
    let mut vm = compile(
        &[
            (
                "example:active_true",
                "return run execute if predicate example:active_true\n",
            ),
            (
                "example:active_false",
                "return run execute unless predicate example:active_false\n",
            ),
            (
                "example:inverted",
                "return run execute if predicate example:inverted\n",
            ),
            (
                "example:all_short_circuit",
                "return run execute if predicate example:all_short_circuit\n",
            ),
            (
                "example:any_short_circuit",
                "return run execute if predicate example:any_short_circuit\n",
            ),
            (
                "example:modifier_short_circuit",
                "return run execute if predicate example:falsehood if predicate example:active_true\n",
            ),
        ],
        &[],
        &[],
        &[
            (
                "example:active_true",
                r#"{"type":"enchantment_active_check","active":true}"#,
            ),
            (
                "example:active_false",
                r#"{"type":"enchantment_active_check","active":false}"#,
            ),
            ("example:truth", r#"{"type":"survives_explosion"}"#),
            ("example:falsehood", r#"{"type":"killed_by_player"}"#),
            (
                "example:inverted",
                r#"{"type":"inverted","term":"example:active_true"}"#,
            ),
            (
                "example:all_short_circuit",
                r#"{"type":"all_of","terms":["example:falsehood","example:active_true"]}"#,
            ),
            (
                "example:any_short_circuit",
                r#"{"type":"any_of","terms":["example:truth","example:active_true"]}"#,
            ),
        ],
        &[],
    );

    for function in [
        "example:active_true",
        "example:active_false",
        "example:inverted",
    ] {
        match vm
            .execute_function(function, None, context(), LIMIT, drop)
            .into_result()
        {
            Err(ExecutionError::PredicateEvaluationFailed { reason }) => {
                assert!(reason.contains("enchantment_active"), "{reason}");
            }
            result => panic!("expected a predicate evaluation error, got {result:?}"),
        }
    }
    assert_function(&mut vm, "example:all_short_circuit", returned(false, 0));
    assert_function(&mut vm, "example:any_short_circuit", returned(true, 1));
    assert_function(
        &mut vm,
        "example:modifier_short_circuit",
        returned(false, 0),
    );
}

#[test]
fn context_absent_predicate_codecs_are_validated_at_load_time() {
    for entity in [
        "this",
        "attacker",
        "direct_attacker",
        "attacking_player",
        "target_entity",
        "interacting_entity",
    ] {
        let source = format!(r#"{{"type":"entity_scores","scores":{{}},"entity":"{entity}"}}"#);
        resources(&[], &[], &[], &[("example:test", &source)], &[]).unwrap();
    }

    for source in [
        r#"{"type":"match_tool","predicate":{"unknown":1}}"#,
        r#"{"type":"damage_source_properties","predicate":{"unknown":1}}"#,
    ] {
        resources(&[], &[], &[], &[("example:test", source)], &[]).unwrap();
    }

    for (source, expected) in [
        (
            r#"{"type":"entity_scores","scores":{"points":{"min":"example:missing"}},"entity":"this"}"#,
            "provider `example:missing` does not exist",
        ),
        (
            r#"{"type":"entity_scores","scores":{},"entity":"invalid"}"#,
            "invalid entity target",
        ),
        (
            r#"{"type":"weather_check","raining":true}"#,
            "physical-world weather",
        ),
        (
            r#"{"type":"weather_check","raining":1}"#,
            "must be a boolean",
        ),
        (
            r#"{"type":"enchantment_active_check"}"#,
            "missing field `active`",
        ),
        (
            r#"{"type":"enchantment_active_check","active":1}"#,
            "must be a boolean",
        ),
    ] {
        let error = resources(&[], &[], &[], &[("example:test", source)], &[]).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
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
            "execute if predicate example:test\nreturn run compute default float {type:uniform,min:0,max:10}\n",
        )],
        &[],
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
        .execute_function("example:main", None, context(), LIMIT, drop)
        .into_result()
        .unwrap()
    {
        ExecutionOutcome::Result {
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
                "return run compute default float {type:uniform,min:0,max:10}\n",
            ),
        ],
        &[],
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
        &[],
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
        &[],
        &[
            ("example:truth", r#"{"type":"all_of","terms":[]}"#),
            ("example:falsehood", r#"{"type":"any_of","terms":[]}"#),
        ],
        &[],
    );

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
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
                "return run compute default integer example:conditional_true\n",
            ),
            (
                "example:conditional_false",
                "return run compute default integer example:conditional_false\n",
            ),
            (
                "example:conditional_default",
                "return run compute default integer example:conditional_default\n",
            ),
            (
                "example:inline_conditional",
                "return run compute default integer {type:conditional,condition:{type:all_of,terms:[]},on_true:6}\n",
            ),
            (
                "example:dispatcher",
                "return run compute default integer example:dispatcher\n",
            ),
            (
                "example:dispatcher_default",
                "return run compute default integer example:dispatcher_default\n",
            ),
            (
                "example:dispatcher_implicit",
                "return run compute default integer example:dispatcher_implicit\n",
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
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","value":100},{"condition":"example:truth","value":4},{"condition":"example:truth","value":9}],"default":11}"#,
            ),
            (
                "example:dispatcher_default",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","value":100}],"default":11}"#,
            ),
            (
                "example:dispatcher_implicit",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","value":100}]}"#,
            ),
        ],
        &[],
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
                "return run compute default integer example:conditional\n",
            ),
            (
                "example:next",
                "return run compute default float example:uniform\n",
            ),
        ],
        &[
            ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
            (
                "example:conditional",
                r#"{"type":"conditional","condition":"example:falsehood","on_true":"example:uniform","on_false":4}"#,
            ),
        ],
        &[("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#)],
        &predicates,
        &[],
    );
    assert_function(&mut conditional, "example:conditional", returned(true, 4));
    assert_function(&mut conditional, "example:next", returned(true, 7));

    let mut dispatcher = compile(
        &[
            (
                "example:dispatcher",
                "return run compute default integer example:dispatcher\n",
            ),
            (
                "example:next",
                "return run compute default float example:uniform\n",
            ),
        ],
        &[
            ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
            (
                "example:dispatcher",
                r#"{"type":"number_dispatcher","cases":[{"condition":"example:falsehood","value":"example:uniform"},{"condition":"example:truth","value":4},{"condition":"example:random","value":5}],"default":6}"#,
            ),
        ],
        &[("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#)],
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
            r#"{"type":"time_check","clock":"minecraft:day_time","value":0}"#,
            "outside Worldless scope",
        ),
        (
            "example:unreachable_unsupported",
            r#"{"type":"all_of","terms":[{"type":"any_of","terms":[]},{"type":"time_check","clock":"minecraft:day_time","value":0}]}"#,
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
        let error = resources(&[], &[], &[], &[(id, source)], &[]).unwrap_err();
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
        &[],
        &[(
            "example:predicate",
            r#"{"type":"int_value_check","value":"example:provider","test":1}"#,
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
            &[],
            &[("example:truth", r#"{"type":"all_of","terms":[]}"#)],
            &[("example:predicates", r#"{"values":["example:truth"]}"#)],
        )
        .unwrap_err();
        assert!(matches!(error, LoadError::InvalidFunction { .. }));
    }
}
