mod common;

use common::context;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{
    ExecutionError, ExecutionOutcome, LoadError, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 512;
static NEXT_PACK: AtomicU64 = AtomicU64::new(0);

struct TestPack {
    root: PathBuf,
}

impl TestPack {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-compute-test-{}-{}",
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

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn compile(functions: &[(&str, &str)], providers: &[(&str, &str)]) -> Vm {
    compile_with_tags(functions, providers, &[])
}

fn compile_with_tags(
    functions: &[(&str, &str)],
    providers: &[(&str, &str)],
    provider_tags: &[(&str, &str)],
) -> Vm {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let providers = providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::NumberProvider, *id, *source));
    let provider_tags = provider_tags
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::NumberProviderTag, *id, *source));
    load_memory(functions.chain(providers).chain(provider_tags)).unwrap()
}

fn load_memory(resources: impl IntoIterator<Item = MemoryResource>) -> Result<Vm, LoadError> {
    Vm::from_packs([Pack::memory(resources)], 0)
}

fn assert_function(vm: &mut Vm, function: &str, expected: ExecutionOutcome) {
    assert_eq!(
        vm.execute_function(function, None, context(), LIMIT)
            .unwrap(),
        expected,
        "{function}"
    );
}

#[test]
fn resource_or_inline_parsing_is_identifier_first_and_modes_match_minecraft() {
    let mut vm = compile(
        &[
            ("example:named", "return run compute default minecraft:1\n"),
            (
                "example:default_namespace",
                "return run compute default 1\n",
            ),
            (
                "example:inline",
                "return run compute default {type:constant,value:1.5}\n",
            ),
            (
                "example:inline_integer",
                "return run compute default {type:constant,value:1.5} integer\n",
            ),
            (
                "example:negative",
                "return run compute default {type:constant,value:-1.5}\n",
            ),
            (
                "example:negative_integer",
                "return run compute default {type:constant,value:-1.5} integer\n",
            ),
            (
                "example:scaled",
                "return run compute default {type:constant,value:1.75} 2\n",
            ),
            ("example:signed_literal", "return run compute default +1\n"),
        ],
        &[("1", "9")],
    );

    assert_function(&mut vm, "example:named", returned(true, 9));
    assert_function(&mut vm, "example:default_namespace", returned(true, 9));
    assert_function(&mut vm, "example:inline", returned(true, 1));
    assert_function(&mut vm, "example:inline_integer", returned(true, 2));
    assert_function(&mut vm, "example:negative", returned(true, -2));
    assert_function(&mut vm, "example:negative_integer", returned(true, -1));
    assert_function(&mut vm, "example:scaled", returned(true, 3));
    assert_function(&mut vm, "example:signed_literal", returned(true, 1));

    let error = load_memory([MemoryResource::new(
        ResourceKind::Function,
        "example:missing",
        "return run compute default 1\n",
    )])
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::InvalidFunction { reason, .. }
            if reason.contains("number provider `minecraft:1` does not exist")
    ));
}

#[test]
fn fixed_score_and_storage_providers_use_their_java_numeric_conversions() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #value state 7\ndata merge storage example:state {nested:{value:-1.75f},values:[1,2],text:\"not a number\"}\n",
            ),
            (
                "example:score_float",
                "return run compute default example:score\n",
            ),
            (
                "example:score_integer",
                "return run compute default example:score integer\n",
            ),
            (
                "example:storage_float",
                "return run compute default example:storage\n",
            ),
            (
                "example:storage_integer",
                "return run compute default example:storage integer\n",
            ),
            (
                "example:missing_storage",
                "return run compute default example:missing_storage integer\n",
            ),
            (
                "example:multiple_storage",
                "return run compute default example:multiple_storage\n",
            ),
            (
                "example:nonnumeric_storage",
                "return run compute default example:nonnumeric_storage\n",
            ),
            (
                "example:prefix_storage_path",
                "return run compute default example:prefix_storage_path integer\n",
            ),
            (
                "example:empty_storage_path",
                "return run compute default example:empty_storage_path integer\n",
            ),
            (
                "example:missing_score_with_infinite_scale",
                "return run compute default example:missing_score_with_infinite_scale\n",
            ),
        ],
        &[
            (
                "example:score",
                r##"{"type":"score","target":{"type":"fixed","name":"#value"},"score":"state","scale":0.5}"##,
            ),
            (
                "example:storage",
                r#"{"type":"storage","storage":"example:state","path":"nested.value"}"#,
            ),
            (
                "example:missing_storage",
                r#"{"type":"storage","storage":"example:missing","path":"value"}"#,
            ),
            (
                "example:multiple_storage",
                r#"{"type":"storage","storage":"example:state","path":"values[]"}"#,
            ),
            (
                "example:nonnumeric_storage",
                r#"{"type":"storage","storage":"example:state","path":"text"}"#,
            ),
            (
                "example:prefix_storage_path",
                r#"{"type":"storage","storage":"example:state","path":"nested.value ignored"}"#,
            ),
            (
                "example:empty_storage_path",
                r#"{"type":"storage","storage":"example:state","path":""}"#,
            ),
            (
                "example:missing_score_with_infinite_scale",
                r##"{"type":"sum","operands":[{"type":"score","target":{"type":"fixed","name":"#missing"},"score":"missing","scale":1e400},1]}"##,
            ),
        ],
    );

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:score_float", returned(true, 3));
    assert_function(&mut vm, "example:score_integer", returned(true, 4));
    assert_function(&mut vm, "example:storage_float", returned(true, -2));
    assert_function(&mut vm, "example:storage_integer", returned(true, -1));
    assert_function(&mut vm, "example:missing_storage", returned(true, 0));
    assert_function(&mut vm, "example:multiple_storage", returned(true, 0));
    assert_function(&mut vm, "example:nonnumeric_storage", returned(true, 0));
    assert_function(&mut vm, "example:prefix_storage_path", returned(true, -1));
    assert_function(&mut vm, "example:empty_storage_path", returned(true, 0));
    assert_function(
        &mut vm,
        "example:missing_score_with_infinite_scale",
        returned(true, 1),
    );
}

#[test]
fn inline_snbt_collection_tags_decode_as_provider_lists() {
    let mut vm = compile(
        &[
            (
                "example:bytes",
                "return run compute default {type:sum,operands:[B;1b,2b]} integer\n",
            ),
            (
                "example:ints",
                "return run compute default {type:sum,operands:[I;1,2]} integer\n",
            ),
            (
                "example:longs",
                "return run compute default {type:sum,operands:[L;1l,2l]} integer\n",
            ),
            (
                "example:dispatcher",
                "return run compute default {type:number_dispatcher,cases:[B;],default:4} integer\n",
            ),
        ],
        &[],
    );

    for function in ["example:bytes", "example:ints", "example:longs"] {
        assert_function(&mut vm, function, returned(true, 3));
    }
    assert_function(&mut vm, "example:dispatcher", returned(true, 4));
}

#[test]
fn json_lone_surrogates_are_preserved_in_fixed_score_targets_and_storage_paths() {
    let mut vm = compile(
        &[
            (
                "example:set_score",
                "$scoreboard players set $(holder) values 7\n",
            ),
            (
                "example:setup",
                "scoreboard objectives add values dummy\nfunction example:set_score {holder:\"#\\uD800\"}\ndata merge storage example:state {\"\\uD800\":9}\n",
            ),
            (
                "example:score",
                "return run compute default example:surrogate_score integer\n",
            ),
            (
                "example:storage",
                "return run compute default example:surrogate_storage integer\n",
            ),
        ],
        &[
            (
                "example:surrogate_score",
                r##"{"type":"score","target":{"type":"fixed","name":"#\uD800"},"score":"values"}"##,
            ),
            (
                "example:surrogate_storage",
                r#"{"type":"storage","storage":"example:state","path":"\"\uD800\""}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:score", returned(true, 7));
    assert_function(&mut vm, "example:storage", returned(true, 9));
}

#[test]
fn aggregates_use_separate_float_and_integer_evaluation_and_java_empty_values() {
    let mut vm = compile(
        &[
            (
                "example:sum_float",
                "return run compute default example:sum\n",
            ),
            (
                "example:sum_integer",
                "return run compute default example:sum integer\n",
            ),
            (
                "example:product_float",
                "return run compute default example:product\n",
            ),
            (
                "example:product_integer",
                "return run compute default example:product integer\n",
            ),
            (
                "example:minimum",
                "return run compute default example:minimum integer\n",
            ),
            (
                "example:maximum",
                "return run compute default example:maximum integer\n",
            ),
            (
                "example:average_float",
                "return run compute default example:average\n",
            ),
            (
                "example:average_integer",
                "return run compute default example:average integer\n",
            ),
            (
                "example:empty_sum",
                "return run compute default {type:sum,operands:[]} integer\n",
            ),
            (
                "example:empty_product",
                "return run compute default {type:product,operands:[]} integer\n",
            ),
            (
                "example:empty_min_float",
                "return run compute default {type:minimum,operands:[]}\n",
            ),
            (
                "example:empty_min_integer",
                "return run compute default {type:minimum,operands:[]} integer\n",
            ),
            (
                "example:empty_max_float",
                "return run compute default {type:maximum,operands:[]}\n",
            ),
            (
                "example:empty_max_integer",
                "return run compute default {type:maximum,operands:[]} integer\n",
            ),
            (
                "example:empty_average",
                "return run compute default {type:average,operands:[]} integer\n",
            ),
        ],
        &[
            ("example:sum", r#"{"type":"sum","operands":[0.6,0.6]}"#),
            (
                "example:product",
                r#"{"type":"product","operands":[1.5,2]}"#,
            ),
            (
                "example:minimum",
                r#"{"type":"minimum","operands":[1.4,1.6]}"#,
            ),
            (
                "example:maximum",
                r#"{"type":"maximum","operands":[1.4,1.6]}"#,
            ),
            (
                "example:average",
                r#"{"type":"average","operands":[0.6,0.6]}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:sum_float", returned(true, 1));
    assert_function(&mut vm, "example:sum_integer", returned(true, 2));
    assert_function(&mut vm, "example:product_float", returned(true, 3));
    assert_function(&mut vm, "example:product_integer", returned(true, 4));
    assert_function(&mut vm, "example:minimum", returned(true, 1));
    assert_function(&mut vm, "example:maximum", returned(true, 2));
    assert_function(&mut vm, "example:average_float", returned(true, 0));
    assert_function(&mut vm, "example:average_integer", returned(true, 1));
    assert_function(&mut vm, "example:empty_sum", returned(true, 0));
    assert_function(&mut vm, "example:empty_product", returned(true, 1));
    assert_function(&mut vm, "example:empty_min_float", returned(true, i32::MAX));
    assert_function(
        &mut vm,
        "example:empty_min_integer",
        returned(true, i32::MAX),
    );
    assert_function(&mut vm, "example:empty_max_float", returned(true, i32::MIN));
    assert_function(
        &mut vm,
        "example:empty_max_integer",
        returned(true, -i32::MAX),
    );
    assert_function(&mut vm, "example:empty_average", returned(true, 0));
}

#[test]
fn random_providers_are_deterministic_and_consume_rng_in_operand_order() {
    let functions = [
        (
            "example:uniform_float",
            "return run compute default example:uniform\n",
        ),
        (
            "example:uniform_integer",
            "return run compute default example:uniform integer\n",
        ),
        (
            "example:ordered_float",
            "return run compute default example:ordered\n",
        ),
        (
            "example:ordered_integer",
            "return run compute default example:ordered integer\n",
        ),
        (
            "example:binomial",
            "return run compute default example:binomial integer\n",
        ),
        (
            "example:weighted",
            "return run compute default example:weighted\n",
        ),
    ];
    let providers = [
        ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
        (
            "example:ordered",
            r#"{"type":"sum","operands":[{"type":"uniform","min":0,"max":10},{"type":"uniform","min":0,"max":100}]}"#,
        ),
        ("example:binomial", r#"{"type":"binomial","n":5,"p":0.5}"#),
        (
            "example:weighted",
            r#"{"type":"weighted_list","distribution":[{"data":{"type":"uniform","min":0,"max":10},"weight":1},{"data":20,"weight":9}]}"#,
        ),
    ];

    let mut float_a = compile(&functions, &providers);
    let mut float_b = compile(&functions, &providers);
    for expected in [7, 8, 2] {
        assert_function(
            &mut float_a,
            "example:uniform_float",
            returned(true, expected),
        );
        assert_function(
            &mut float_b,
            "example:uniform_float",
            returned(true, expected),
        );
    }

    let mut integer = compile(&functions, &providers);
    // Integer uniform includes both endpoints, unlike float uniform's half-open range.
    for expected in [0, 6, 8] {
        assert_function(
            &mut integer,
            "example:uniform_integer",
            returned(true, expected),
        );
    }

    let mut ordered_float = compile(&functions, &providers);
    assert_function(
        &mut ordered_float,
        "example:ordered_float",
        returned(true, 90),
    );
    assert_function(
        &mut ordered_float,
        "example:ordered_float",
        returned(true, 63),
    );

    let mut ordered_integer = compile(&functions, &providers);
    assert_function(
        &mut ordered_integer,
        "example:ordered_integer",
        returned(true, 72),
    );
    assert_function(
        &mut ordered_integer,
        "example:ordered_integer",
        returned(true, 13),
    );

    let mut binomial = compile(&functions, &providers);
    assert_function(&mut binomial, "example:binomial", returned(true, 1));
    assert_function(&mut binomial, "example:binomial", returned(true, 2));

    let mut weighted = compile(&functions, &providers);
    assert_function(&mut weighted, "example:weighted", returned(true, 8));
    assert_function(&mut weighted, "example:weighted", returned(true, 20));
}

#[test]
fn data_compute_sources_cover_every_storage_modify_operation() {
    let mut vm = compile(
        &[
            (
                "example:setup",
                "data merge storage example:data {scalar:0,list:[0.0f],obj:{kept:1}}\n",
            ),
            (
                "example:set_float",
                "return run data modify storage example:data scalar set compute default {type:constant,value:1.75}\n",
            ),
            (
                "example:verify_float",
                "execute if data storage example:data {scalar:1.75f} run return 1\nreturn fail\n",
            ),
            (
                "example:set_integer",
                "return run data modify storage example:data scalar set compute default {type:constant,value:1.5} integer\n",
            ),
            (
                "example:append",
                "return run data modify storage example:data list append compute default {type:constant,value:3.25}\n",
            ),
            (
                "example:prepend",
                "return run data modify storage example:data list prepend compute default {type:constant,value:1.25}\n",
            ),
            (
                "example:insert",
                "return run data modify storage example:data list insert 1 compute default {type:constant,value:2.25}\n",
            ),
            (
                "example:merge",
                "return run data modify storage example:data obj merge compute default {type:constant,value:9}\n",
            ),
            (
                "example:verify_final",
                "execute if data storage example:data {scalar:2,list:[1.25f,2.25f,0.0f,3.25f],obj:{kept:1}} run return 1\nreturn fail\n",
            ),
        ],
        &[],
    );

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:set_float", returned(true, 1));
    assert_function(&mut vm, "example:verify_float", returned(true, 1));
    assert_function(&mut vm, "example:set_integer", returned(true, 1));
    assert_function(&mut vm, "example:append", returned(true, 1));
    assert_function(&mut vm, "example:prepend", returned(true, 1));
    assert_function(&mut vm, "example:insert", returned(true, 1));
    assert_function(&mut vm, "example:merge", returned(false, 0));
    assert_function(&mut vm, "example:verify_final", returned(true, 1));
}

#[test]
fn named_references_and_provider_tags_resolve_in_declared_order() {
    let mut vm = compile_with_tags(
        &[
            (
                "example:direct",
                "return run compute default example:direct integer\n",
            ),
            (
                "example:tagged",
                "return run compute default example:tagged integer\n",
            ),
            (
                "example:builtin",
                "return run compute default minecraft:brewing/uses_default integer\n",
            ),
        ],
        &[
            ("example:one", "1.25"),
            ("example:two", "2.25"),
            (
                "example:direct",
                r#"{"type":"sum","operands":["example:one","example:two"]}"#,
            ),
            (
                "example:tagged",
                r##"{"type":"sum","operands":"#example:all"}"##,
            ),
        ],
        &[
            ("example:tail", r#"{"values":["example:two"]}"#),
            (
                "example:all",
                r##"{"values":["example:one",{"id":"example:missing","required":false},"#example:tail"]}"##,
            ),
        ],
    );

    assert_function(&mut vm, "example:direct", returned(true, 3));
    assert_function(&mut vm, "example:tagged", returned(true, 3));
    assert_function(&mut vm, "example:builtin", returned(true, 20));
}

#[test]
fn provider_tag_json_uses_minecrafts_utf16_and_nesting_boundaries() {
    let mut ignored = "0".to_owned();
    for _ in 0..130 {
        ignored = format!("[{ignored}]");
    }
    let tag = format!(
        "\u{feff}{{\"values\":[\"example:one\"],\"ignored_surrogate\":\"\\uD800\",\"ignored_nested\":{ignored}}}"
    );
    let mut vm = compile_with_tags(
        &[(
            "example:main",
            "return run compute default example:tagged integer\n",
        )],
        &[
            ("example:one", "1"),
            (
                "example:tagged",
                r##"{"type":"sum","operands":"#example:compat"}"##,
            ),
        ],
        &[("example:compat", tag.as_str())],
    );

    assert_function(&mut vm, "example:main", returned(true, 1));
}

#[test]
fn directory_loader_reads_number_provider_resources_and_tags() {
    let pack = TestPack::new();
    pack.write(
        "data/example/function/main.mcfunction",
        "return run compute default example:tagged integer\n",
    );
    pack.write("data/example/number_provider/one.json", "1.25");
    pack.write("data/example/number_provider/two.json", "2.25");
    pack.write(
        "data/example/number_provider/tagged.json",
        r##"{"type":"sum","operands":"#example:values"}"##,
    );
    pack.write(
        "data/example/tags/number_provider/values.json",
        r#"{"values":["example:one","example:two"]}"#,
    );

    let mut vm = Vm::from_packs([Pack::directory(pack.root())], 0).unwrap();
    assert_function(&mut vm, "example:main", returned(true, 3));
}

#[test]
fn directory_loader_applies_a_worldless_override_of_the_fast_cooking_predicate() {
    let pack = TestPack::new();
    pack.write(
        "data/minecraft/predicate/block/fast_cooking.json",
        r#"{"type":"minecraft:all_of","terms":[]}"#,
    );
    pack.write(
        "data/example/function/burn_time.mcfunction",
        "return run compute default minecraft:cooking/time_bamboo\n",
    );
    pack.write(
        "data/example/function/speed.mcfunction",
        "return run compute default minecraft:cooking/speed_default integer\n",
    );

    let mut vm = Vm::from_packs([Pack::directory(pack.root())], 0).unwrap();
    assert_function(&mut vm, "example:burn_time", returned(true, 25));
    assert_function(&mut vm, "example:speed", returned(true, 2));
}

#[test]
fn an_empty_number_dispatcher_uses_its_default_without_a_predicate_context() {
    let mut vm = compile(
        &[
            (
                "example:explicit_float",
                "return run compute default example:explicit\n",
            ),
            (
                "example:explicit_integer",
                "return run compute default example:explicit integer\n",
            ),
            (
                "example:implicit",
                "return run compute default example:implicit integer\n",
            ),
        ],
        &[
            ("example:value", "2.5"),
            (
                "example:explicit",
                r#"{"type":"number_dispatcher","cases":[],"default":"example:value"}"#,
            ),
            (
                "example:implicit",
                r#"{"type":"number_dispatcher","cases":[]}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:explicit_float", returned(true, 2));
    assert_function(&mut vm, "example:explicit_integer", returned(true, 3));
    assert_function(&mut vm, "example:implicit", returned(true, 0));
}

#[test]
fn vanilla_number_providers_have_their_default_context_projection() {
    let providers = [
        ("compostable/low", 0),
        ("compostable/low_medium", 1),
        ("compostable/medium", 1),
        ("compostable/medium_high", 1),
        ("compostable/always_add_one", 1),
        ("cooking/time_bamboo", 50),
        ("cooking/time_wool_slabs", 50),
        ("cooking/time_wool_carpets", 67),
        ("cooking/time_dry_plants", 100),
        ("cooking/time_wood_items_extra_small", 100),
        ("cooking/time_wool", 100),
        ("cooking/time_wood_slabs", 150),
        ("cooking/time_wood_items_large", 200),
        ("cooking/time_roots", 300),
        ("cooking/time_wood_blocks", 300),
        ("cooking/time_wood_items_small", 300),
        ("cooking/time_hanging_signs", 800),
        ("cooking/time_boats", 1_200),
        ("cooking/time_coal", 1_600),
        ("cooking/time_blaze_rod", 2_400),
        ("cooking/time_dried_kelp_block", 4_001),
        ("cooking/time_coal_block", 16_000),
        ("cooking/time_lava_bucket", 20_000),
        ("cooking/speed_default", 1),
        ("cooking/normal_speed_multiplier", 1),
        ("cooking/fast_speed_multiplier", 2),
        ("cooking/normal_burn_time_multiplier", 1),
        ("cooking/fast_burn_time_multiplier", 1),
        ("brewing/speed_default", 1),
        ("brewing/uses_default", 20),
    ];
    let functions = providers
        .iter()
        .enumerate()
        .map(|(index, (provider, _))| {
            (
                format!("example:builtin_{index}"),
                format!("return run compute default minecraft:{provider} integer\n"),
            )
        })
        .collect::<Vec<_>>();
    let mut vm = load_memory(
        functions
            .iter()
            .map(|(id, source)| MemoryResource::new(ResourceKind::Function, id, source)),
    )
    .unwrap();

    for (index, (_, expected)) in providers.into_iter().enumerate() {
        assert_function(
            &mut vm,
            &format!("example:builtin_{index}"),
            returned(true, expected),
        );
    }
}

#[test]
fn invalid_references_shapes_and_out_of_scope_contexts_are_rejected() {
    for (id, provider, expected) in [
        (
            "example:missing",
            r#"{"type":"sum","operands":"example:absent"}"#,
            "does not exist",
        ),
        (
            "example:weight",
            r#"{"type":"weighted_list","distribution":[{"data":1,"weight":0}]}"#,
            "non-zero weight",
        ),
        (
            "example:conditional",
            r#"{"type":"conditional"}"#,
            "missing field `condition`",
        ),
        (
            "example:dispatcher_with_case",
            r#"{"type":"number_dispatcher","cases":[{"condition":"example:predicate","number_provider":1}],"default":0}"#,
            "does not exist",
        ),
        (
            "example:environment",
            r#"{"type":"environment_attribute"}"#,
            "physical-world loot context",
        ),
        (
            "example:context_score",
            r#"{"type":"score","target":"this","score":"value"}"#,
            "outside Worldless scope",
        ),
    ] {
        let error = load_memory([MemoryResource::new(
            ResourceKind::NumberProvider,
            id,
            provider,
        )])
        .unwrap_err();
        assert!(matches!(
            error,
            LoadError::InvalidNumberProvider { reason, .. }
                if reason.contains(expected)
        ));
    }

    let cycle = load_memory([
        MemoryResource::new(
            ResourceKind::NumberProvider,
            "example:first",
            r#"{"type":"sum","operands":"example:second"}"#,
        ),
        MemoryResource::new(
            ResourceKind::NumberProvider,
            "example:second",
            r#"{"type":"sum","operands":"example:first"}"#,
        ),
    ])
    .unwrap_err();
    assert!(matches!(
        cycle,
        LoadError::InvalidNumberProvider { reason, .. } if reason.contains("cyclic")
    ));

    let builtin_fast_branch_cycle = load_memory([MemoryResource::new(
        ResourceKind::NumberProvider,
        "minecraft:cooking/fast_burn_time_multiplier",
        r#"{"type":"sum","operands":"minecraft:cooking/time_bamboo"}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        builtin_fast_branch_cycle,
        LoadError::InvalidNumberProvider { reason, .. } if reason.contains("cyclic")
    ));

    let missing_tag_entry = load_memory([MemoryResource::new(
        ResourceKind::NumberProviderTag,
        "example:bad",
        r#"{"values":["example:missing"]}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        missing_tag_entry,
        LoadError::InvalidNumberProviderTag { reason, .. } if reason.contains("does not exist")
    ));

    for source in [
        "return run compute block ~ ~ ~ {type:constant,value:1}\n",
        "return run compute default {type:conditional}\n",
        "return run compute default {type:score,target:this,score:value}\n",
        "return run data modify block ~ ~ ~ value set compute default {type:constant,value:1}\n",
    ] {
        let error = load_memory([MemoryResource::new(
            ResourceKind::Function,
            "example:invalid",
            source,
        )])
        .unwrap_err();
        assert!(matches!(error, LoadError::InvalidFunction { .. }));
    }
}

#[test]
fn resource_aggregates_with_empty_operands_are_loaded_despite_vanillas_warning() {
    let mut vm = compile(
        &[
            (
                "example:sum",
                "return run compute default example:empty_sum integer\n",
            ),
            (
                "example:product",
                "return run compute default example:empty_product integer\n",
            ),
            (
                "example:minimum",
                "return run compute default example:empty_minimum integer\n",
            ),
            (
                "example:maximum",
                "return run compute default example:empty_maximum integer\n",
            ),
            (
                "example:average",
                "return run compute default example:empty_average integer\n",
            ),
        ],
        &[
            ("example:empty_sum", r#"{"type":"sum","operands":[]}"#),
            (
                "example:empty_product",
                r#"{"type":"product","operands":[]}"#,
            ),
            (
                "example:empty_minimum",
                r#"{"type":"minimum","operands":[]}"#,
            ),
            (
                "example:empty_maximum",
                r#"{"type":"maximum","operands":[]}"#,
            ),
            (
                "example:empty_average",
                r#"{"type":"average","operands":[]}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:sum", returned(true, 0));
    assert_function(&mut vm, "example:product", returned(true, 1));
    assert_function(&mut vm, "example:minimum", returned(true, i32::MAX));
    assert_function(&mut vm, "example:maximum", returned(true, -i32::MAX));
    assert_function(&mut vm, "example:average", returned(true, 0));
}

#[test]
fn invalid_uniform_integer_range_aborts_the_execution_queue() {
    let mut vm = compile(
        &[
            (
                "example:overflow",
                "scoreboard objectives add state dummy\nscoreboard players set #after state 0\ncompute default {type:uniform,min:-2147483648,max:2147483647} integer\nscoreboard players set #after state 1\n",
            ),
            (
                "example:read_after",
                "return run scoreboard players get #after state\n",
            ),
        ],
        &[],
    );

    assert!(matches!(
        vm.execute_function("example:overflow", None, context(), LIMIT),
        Err(ExecutionError::NumberProviderEvaluationFailed { reason })
            if reason.contains("bound must be positive")
    ));
    assert_function(&mut vm, "example:read_after", returned(true, 0));
}

#[test]
fn macro_instantiation_uses_the_same_provider_registry_and_inline_parser() {
    let mut vm = compile(
        &[
            (
                "example:inline_macro",
                "$return run compute default {type:constant,value:$(value)} integer\n",
            ),
            (
                "example:named_macro",
                "$return run compute default example:$(provider)\n",
            ),
            (
                "example:inline_call",
                "return run function example:inline_macro {value:-1.5f}\n",
            ),
            (
                "example:named_call",
                "return run function example:named_macro {provider:\"value\"}\n",
            ),
            (
                "example:bad_call",
                "return run function example:named_macro {provider:\"missing\"}\n",
            ),
        ],
        &[("example:value", "6.75")],
    );

    assert_function(&mut vm, "example:inline_call", returned(true, -1));
    assert_function(&mut vm, "example:named_call", returned(true, 6));
    assert_function(&mut vm, "example:bad_call", ExecutionOutcome::NoResult);
}
