mod common;

use common::context;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{
    CompiledProgram, ExecutionError, ExecutionOutcome, LoadError, MemoryResource, Pack,
    ResourceKind, Vm,
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

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

fn compile_int(functions: &[(&str, &str)], providers: &[(&str, &str)]) -> Vm {
    compile_typed(functions, providers, &[])
}

fn compile_float(functions: &[(&str, &str)], providers: &[(&str, &str)]) -> Vm {
    compile_typed(functions, &[], providers)
}

fn compile_typed(
    functions: &[(&str, &str)],
    int_providers: &[(&str, &str)],
    float_providers: &[(&str, &str)],
) -> Vm {
    compile_with_tags(functions, int_providers, &[], float_providers, &[])
}

fn compile_int_with_tags(
    functions: &[(&str, &str)],
    int_providers: &[(&str, &str)],
    int_provider_tags: &[(&str, &str)],
) -> Vm {
    compile_with_tags(functions, int_providers, int_provider_tags, &[], &[])
}

fn compile_with_tags(
    functions: &[(&str, &str)],
    int_providers: &[(&str, &str)],
    int_provider_tags: &[(&str, &str)],
    float_providers: &[(&str, &str)],
    float_provider_tags: &[(&str, &str)],
) -> Vm {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let int_providers = int_providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::ContextIntProvider, *id, *source));
    let int_provider_tags = int_provider_tags
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::ContextIntProviderTag, *id, *source));
    let float_providers = float_providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::ContextFloatProvider, *id, *source));
    let float_provider_tags = float_provider_tags.iter().map(|(id, source)| {
        MemoryResource::new(ResourceKind::ContextFloatProviderTag, *id, *source)
    });
    load_memory(
        functions
            .chain(int_providers)
            .chain(int_provider_tags)
            .chain(float_providers)
            .chain(float_provider_tags),
    )
    .unwrap()
}

fn load_memory(resources: impl IntoIterator<Item = MemoryResource>) -> Result<Vm, LoadError> {
    CompiledProgram::from_packs([Pack::memory(resources)]).map(|program| program.create_vm(0))
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
fn resource_or_inline_parsing_is_identifier_first_and_modes_match_minecraft() {
    let mut vm = compile_typed(
        &[
            (
                "example:named",
                "return run compute default float minecraft:1\n",
            ),
            (
                "example:default_namespace",
                "return run compute default float 1\n",
            ),
            (
                "example:inline",
                "return run compute default float {type:constant,value:1.5}\n",
            ),
            (
                "example:inline_integer",
                "return run compute default integer {type:constant,value:2}\n",
            ),
            (
                "example:negative",
                "return run compute default float {type:constant,value:-1.5}\n",
            ),
            (
                "example:negative_integer",
                "return run compute default integer {type:constant,value:-1}\n",
            ),
            (
                "example:scaled",
                "return run compute default float {type:constant,value:1.75} 2\n",
            ),
            (
                "example:signed_literal",
                "return run compute default float +1\n",
            ),
        ],
        &[("1", "9")],
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
        "return run compute default float 1\n",
    )])
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::InvalidFunction { reason, .. }
            if reason.contains("does not exist")
    ));
}

#[test]
fn score_and_typed_storage_providers_use_their_java_numeric_conversions() {
    let mut vm = compile_typed(
        &[
            (
                "example:setup",
                "scoreboard objectives add state dummy\nscoreboard players set #value state 7\ndata merge storage example:state {nested:{value:-1.75f},values:[1,2],text:\"not a number\"}\n",
            ),
            (
                "example:score_scaled",
                "return run compute default float example:score_scaled\n",
            ),
            (
                "example:score_integer",
                "return run compute default integer example:score\n",
            ),
            (
                "example:storage_float",
                "return run compute default float example:storage\n",
            ),
            (
                "example:storage_integer",
                "return run compute default integer example:storage\n",
            ),
            (
                "example:missing_storage",
                "return run compute default integer example:missing_storage\n",
            ),
            (
                "example:multiple_storage",
                "return run compute default float example:multiple_storage\n",
            ),
            (
                "example:nonnumeric_storage",
                "return run compute default float example:nonnumeric_storage\n",
            ),
            (
                "example:prefix_storage_path",
                "return run compute default integer example:prefix_storage_path\n",
            ),
            (
                "example:empty_storage_path",
                "return run compute default integer example:empty_storage_path\n",
            ),
            (
                "example:missing_score_with_fallback",
                "return run compute default integer example:missing_score_with_fallback\n",
            ),
        ],
        &[
            (
                "example:score",
                r##"{"type":"score","target":{"type":"fixed","name":"#value"},"score":"state"}"##,
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
                "example:prefix_storage_path",
                r#"{"type":"storage","storage":"example:state","path":"nested.value ignored"}"#,
            ),
            (
                "example:empty_storage_path",
                r#"{"type":"storage","storage":"example:state","path":""}"#,
            ),
            (
                "example:missing_score_with_fallback",
                r##"{"type":"score","target":{"type":"fixed","name":"#missing"},"score":"missing","fallback":1}"##,
            ),
        ],
        &[
            (
                "example:score_scaled",
                r#"{"type":"mul","inputs":[{"type":"from_int","input":"example:score"},0.5]}"#,
            ),
            (
                "example:storage",
                r#"{"type":"storage","storage":"example:state","path":"nested.value"}"#,
            ),
            (
                "example:multiple_storage",
                r#"{"type":"storage","storage":"example:state","path":"values[]"}"#,
            ),
            (
                "example:nonnumeric_storage",
                r#"{"type":"storage","storage":"example:state","path":"text"}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:setup", ExecutionOutcome::NoResult);
    assert_function(&mut vm, "example:score_scaled", returned(true, 3));
    assert_function(&mut vm, "example:score_integer", returned(true, 7));
    assert_function(&mut vm, "example:storage_float", returned(true, -2));
    assert_function(&mut vm, "example:storage_integer", returned(true, -1));
    assert_function(&mut vm, "example:missing_storage", returned(true, 0));
    assert_function(&mut vm, "example:multiple_storage", returned(true, 0));
    assert_function(&mut vm, "example:nonnumeric_storage", returned(true, 0));
    assert_function(&mut vm, "example:prefix_storage_path", returned(true, -1));
    assert_function(&mut vm, "example:empty_storage_path", returned(true, 0));
    assert_function(
        &mut vm,
        "example:missing_score_with_fallback",
        returned(true, 1),
    );
}

#[test]
fn inline_snbt_collection_tags_decode_as_provider_lists() {
    let mut vm = compile_int(
        &[
            (
                "example:bytes",
                "return run compute default integer {type:add,inputs:[B;1b,2b]}\n",
            ),
            (
                "example:ints",
                "return run compute default integer {type:add,inputs:[I;1,2]}\n",
            ),
            (
                "example:longs",
                "return run compute default integer {type:add,inputs:[L;1l,2l]}\n",
            ),
            (
                "example:dispatcher",
                "return run compute default integer {type:number_dispatcher,cases:[B;],default:4}\n",
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
    let mut vm = compile_int(
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
                "return run compute default integer example:surrogate_score\n",
            ),
            (
                "example:storage",
                "return run compute default integer example:surrogate_storage\n",
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
fn aggregates_use_separate_float_and_integer_registries() {
    let mut vm = compile_typed(
        &[
            (
                "example:sum_float",
                "return run compute default float example:sum\n",
            ),
            (
                "example:sum_integer",
                "return run compute default integer example:sum\n",
            ),
            (
                "example:product_float",
                "return run compute default float example:product\n",
            ),
            (
                "example:product_integer",
                "return run compute default integer example:product\n",
            ),
            (
                "example:minimum",
                "return run compute default integer example:minimum\n",
            ),
            (
                "example:maximum",
                "return run compute default integer example:maximum\n",
            ),
            (
                "example:average_float",
                "return run compute default float example:average\n",
            ),
            (
                "example:average_integer",
                "return run compute default integer example:average\n",
            ),
        ],
        &[
            ("example:sum", r#"{"type":"add","inputs":[1,1]}"#),
            ("example:product", r#"{"type":"mul","inputs":[2,2]}"#),
            ("example:minimum", r#"{"type":"min","inputs":[1,2]}"#),
            ("example:maximum", r#"{"type":"max","inputs":[1,2]}"#),
            ("example:average", r#"{"type":"avg","inputs":[1,2]}"#),
        ],
        &[
            ("example:sum", r#"{"type":"add","inputs":[0.6,0.6]}"#),
            ("example:product", r#"{"type":"mul","inputs":[1.5,2]}"#),
            ("example:average", r#"{"type":"avg","inputs":[0.6,0.6]}"#),
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
}

#[test]
fn random_providers_are_deterministic_and_consume_rng_in_operand_order() {
    let functions = [
        (
            "example:uniform_float",
            "return run compute default float example:uniform\n",
        ),
        (
            "example:uniform_integer",
            "return run compute default integer example:uniform\n",
        ),
        (
            "example:ordered_float",
            "return run compute default float example:ordered\n",
        ),
        (
            "example:ordered_integer",
            "return run compute default integer example:ordered\n",
        ),
        (
            "example:binomial",
            "return run compute default integer example:binomial\n",
        ),
        (
            "example:weighted",
            "return run compute default float example:weighted\n",
        ),
    ];
    let int_providers = [
        ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
        (
            "example:ordered",
            r#"{"type":"add","inputs":[{"type":"uniform","min":0,"max":10},{"type":"uniform","min":0,"max":100}]}"#,
        ),
        ("example:binomial", r#"{"type":"binomial","n":5,"p":0.5}"#),
    ];
    let float_providers = [
        ("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#),
        (
            "example:ordered",
            r#"{"type":"add","inputs":[{"type":"uniform","min":0,"max":10},{"type":"uniform","min":0,"max":100}]}"#,
        ),
        (
            "example:weighted",
            r#"{"type":"weighted_list","distribution":[{"data":{"type":"uniform","min":0,"max":10},"weight":1},{"data":20,"weight":9}]}"#,
        ),
    ];

    let mut float_a = compile_typed(&functions, &int_providers, &float_providers);
    let mut float_b = compile_typed(&functions, &int_providers, &float_providers);
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

    let mut integer = compile_typed(&functions, &int_providers, &float_providers);
    // Integer uniform includes both endpoints, unlike float uniform's half-open range.
    for expected in [0, 6, 8] {
        assert_function(
            &mut integer,
            "example:uniform_integer",
            returned(true, expected),
        );
    }

    let mut ordered_float = compile_typed(&functions, &int_providers, &float_providers);
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

    let mut ordered_integer = compile_typed(&functions, &int_providers, &float_providers);
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

    let mut binomial = compile_typed(&functions, &int_providers, &float_providers);
    assert_function(&mut binomial, "example:binomial", returned(true, 1));
    assert_function(&mut binomial, "example:binomial", returned(true, 2));

    let mut weighted = compile_typed(&functions, &int_providers, &float_providers);
    assert_function(&mut weighted, "example:weighted", returned(true, 8));
    assert_function(&mut weighted, "example:weighted", returned(true, 20));
}

#[test]
fn data_compute_sources_cover_every_storage_modify_operation() {
    let mut vm = compile_int(
        &[
            (
                "example:setup",
                "data merge storage example:data {scalar:0,list:[0.0f],obj:{kept:1}}\n",
            ),
            (
                "example:set_float",
                "return run data modify storage example:data scalar set compute default float {type:constant,value:1.75}\n",
            ),
            (
                "example:verify_float",
                "execute if data storage example:data {scalar:1.75f} run return 1\nreturn fail\n",
            ),
            (
                "example:set_integer",
                "return run data modify storage example:data scalar set compute default integer {type:constant,value:2}\n",
            ),
            (
                "example:append",
                "return run data modify storage example:data list append compute default float {type:constant,value:3.25}\n",
            ),
            (
                "example:prepend",
                "return run data modify storage example:data list prepend compute default float {type:constant,value:1.25}\n",
            ),
            (
                "example:insert",
                "return run data modify storage example:data list insert 1 compute default float {type:constant,value:2.25}\n",
            ),
            (
                "example:merge",
                "return run data modify storage example:data obj merge compute default float {type:constant,value:9}\n",
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
    let mut vm = compile_int_with_tags(
        &[
            (
                "example:direct",
                "return run compute default integer example:direct\n",
            ),
            (
                "example:tagged",
                "return run compute default integer example:tagged\n",
            ),
            (
                "example:builtin",
                "return run compute default integer minecraft:brewing/uses_default\n",
            ),
        ],
        &[
            ("example:one", "1"),
            ("example:two", "2"),
            (
                "example:direct",
                r#"{"type":"add","inputs":["example:one","example:two"]}"#,
            ),
            (
                "example:tagged",
                r##"{"type":"add","inputs":"#example:all"}"##,
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
    let mut vm = compile_int_with_tags(
        &[(
            "example:main",
            "return run compute default integer example:tagged\n",
        )],
        &[
            ("example:one", "1"),
            (
                "example:tagged",
                r##"{"type":"add","inputs":"#example:compat"}"##,
            ),
        ],
        &[("example:compat", tag.as_str())],
    );

    assert_function(&mut vm, "example:main", returned(true, 1));
}

#[test]
fn directory_loader_reads_context_int_provider_resources_and_tags() {
    let pack = TestPack::new();
    pack.write(
        "data/example/function/main.mcfunction",
        "return run compute default integer example:tagged\n",
    );
    pack.write("data/example/context_int_provider/one.json", "1");
    pack.write("data/example/context_int_provider/two.json", "2");
    pack.write(
        "data/example/context_int_provider/tagged.json",
        r##"{"type":"add","inputs":"#example:values"}"##,
    );
    pack.write(
        "data/example/tags/context_int_provider/values.json",
        r#"{"values":["example:one","example:two"]}"#,
    );

    let mut vm = CompiledProgram::from_packs([Pack::directory(pack.root())])
        .map(|program| program.create_vm(0))
        .unwrap();
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
        "return run compute default integer minecraft:cooking/time_bamboo\n",
    );
    pack.write(
        "data/example/function/speed.mcfunction",
        "return run compute default float minecraft:cooking/speed_default\n",
    );

    let mut vm = CompiledProgram::from_packs([Pack::directory(pack.root())])
        .map(|program| program.create_vm(0))
        .unwrap();
    assert_function(&mut vm, "example:burn_time", returned(true, 25));
    assert_function(&mut vm, "example:speed", returned(true, 2));
}

#[test]
fn an_empty_number_dispatcher_uses_its_default_without_a_predicate_context() {
    let mut vm = compile_typed(
        &[
            (
                "example:explicit_float",
                "return run compute default float example:explicit\n",
            ),
            (
                "example:explicit_integer",
                "return run compute default integer example:explicit\n",
            ),
            (
                "example:implicit",
                "return run compute default integer example:implicit\n",
            ),
        ],
        &[
            ("example:value", "3"),
            (
                "example:explicit",
                r#"{"type":"number_dispatcher","cases":[],"default":"example:value"}"#,
            ),
            (
                "example:implicit",
                r#"{"type":"number_dispatcher","cases":[]}"#,
            ),
        ],
        &[
            ("example:value", "2.5"),
            (
                "example:explicit",
                r#"{"type":"number_dispatcher","cases":[],"default":"example:value"}"#,
            ),
        ],
    );

    assert_function(&mut vm, "example:explicit_float", returned(true, 2));
    assert_function(&mut vm, "example:explicit_integer", returned(true, 3));
    assert_function(&mut vm, "example:implicit", returned(true, 0));
}

#[test]
fn vanilla_context_providers_have_their_default_context_projection() {
    let providers = [
        ("integer", "compostable/low", 0),
        ("integer", "compostable/low_medium", 1),
        ("integer", "compostable/medium", 1),
        ("integer", "compostable/medium_high", 1),
        ("integer", "compostable/always_add_one", 1),
        ("integer", "cooking/time_bamboo", 50),
        ("integer", "cooking/time_wool_slabs", 50),
        ("integer", "cooking/time_wool_carpets", 67),
        ("integer", "cooking/time_dry_plants", 100),
        ("integer", "cooking/time_wood_items_extra_small", 100),
        ("integer", "cooking/time_wool", 100),
        ("integer", "cooking/time_wood_slabs", 150),
        ("integer", "cooking/time_wood_items_large", 200),
        ("integer", "cooking/time_roots", 300),
        ("integer", "cooking/time_wood_blocks", 300),
        ("integer", "cooking/time_wood_items_small", 300),
        ("integer", "cooking/time_hanging_signs", 800),
        ("integer", "cooking/time_boats", 1_200),
        ("integer", "cooking/time_coal", 1_600),
        ("integer", "cooking/time_blaze_rod", 2_400),
        ("integer", "cooking/time_dried_kelp_block", 4_001),
        ("integer", "cooking/time_coal_block", 16_000),
        ("integer", "cooking/time_lava_bucket", 20_000),
        ("integer", "cooking/normal_burn_time_reduction_factor", 1),
        ("integer", "cooking/fast_burn_time_reduction_factor", 2),
        ("integer", "brewing/uses_default", 20),
        ("float", "cooking/speed_default", 1),
        ("float", "cooking/normal_speed_multiplier", 1),
        ("float", "cooking/fast_speed_multiplier", 2),
        ("float", "brewing/speed_default", 1),
    ];
    let functions = providers
        .iter()
        .enumerate()
        .map(|(index, (mode, provider, _))| {
            (
                format!("example:builtin_{index}"),
                format!("return run compute default {mode} minecraft:{provider}\n"),
            )
        })
        .collect::<Vec<_>>();
    let mut vm = load_memory(
        functions
            .iter()
            .map(|(id, source)| MemoryResource::new(ResourceKind::Function, id, source)),
    )
    .unwrap();

    for (index, (_, _, expected)) in providers.into_iter().enumerate() {
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
            r#"{"type":"add","inputs":"example:absent"}"#,
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
            r#"{"type":"number_dispatcher","cases":[{"condition":"example:predicate","value":1}],"default":0}"#,
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
            ResourceKind::ContextIntProvider,
            id,
            provider,
        )])
        .unwrap_err();
        assert!(matches!(
            error,
            LoadError::InvalidContextIntProvider { reason, .. }
                if reason.contains(expected)
        ));
    }

    let cycle = load_memory([
        MemoryResource::new(
            ResourceKind::ContextIntProvider,
            "example:first",
            r#"{"type":"add","inputs":"example:second"}"#,
        ),
        MemoryResource::new(
            ResourceKind::ContextIntProvider,
            "example:second",
            r#"{"type":"add","inputs":"example:first"}"#,
        ),
    ])
    .unwrap_err();
    assert!(matches!(
        cycle,
        LoadError::InvalidContextIntProvider { reason, .. } if reason.contains("cyclic")
    ));

    let builtin_fast_branch_cycle = load_memory([MemoryResource::new(
        ResourceKind::ContextIntProvider,
        "minecraft:cooking/fast_burn_time_reduction_factor",
        r#"{"type":"add","inputs":"minecraft:cooking/time_bamboo"}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        builtin_fast_branch_cycle,
        LoadError::InvalidContextIntProvider { reason, .. } if reason.contains("cyclic")
    ));

    let missing_tag_entry = load_memory([MemoryResource::new(
        ResourceKind::ContextIntProviderTag,
        "example:bad",
        r#"{"values":["example:missing"]}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        missing_tag_entry,
        LoadError::InvalidContextIntProviderTag { reason, .. }
            if reason.contains("does not exist")
    ));

    let missing_float_reference = load_memory([MemoryResource::new(
        ResourceKind::ContextFloatProvider,
        "example:bad",
        r#"{"type":"add","inputs":"example:missing"}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        missing_float_reference,
        LoadError::InvalidContextFloatProvider { reason, .. }
            if reason.contains("does not exist")
    ));

    let missing_float_tag_entry = load_memory([MemoryResource::new(
        ResourceKind::ContextFloatProviderTag,
        "example:bad",
        r#"{"values":["example:missing"]}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        missing_float_tag_entry,
        LoadError::InvalidContextFloatProviderTag { reason, .. }
            if reason.contains("does not exist")
    ));

    for source in [
        "return run compute block ~ ~ ~ {type:constant,value:1}\n",
        "return run compute default float {type:conditional}\n",
        "return run compute default integer {type:score,target:this,score:value}\n",
        "return run data modify block ~ ~ ~ value set compute default float {type:constant,value:1}\n",
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
fn resource_aggregates_require_at_least_one_input() {
    for (id, provider) in [
        ("example:empty_add", r#"{"type":"add","inputs":[]}"#),
        ("example:empty_mul", r#"{"type":"mul","inputs":[]}"#),
        ("example:empty_min", r#"{"type":"min","inputs":[]}"#),
        ("example:empty_max", r#"{"type":"max","inputs":[]}"#),
        ("example:empty_avg", r#"{"type":"avg","inputs":[]}"#),
    ] {
        let error = load_memory([MemoryResource::new(
            ResourceKind::ContextIntProvider,
            id,
            provider,
        )])
        .unwrap_err();
        assert!(matches!(error, LoadError::InvalidContextIntProvider { .. }));
    }

    let error = load_memory([MemoryResource::new(
        ResourceKind::ContextFloatProvider,
        "example:empty_add",
        r#"{"type":"add","inputs":[]}"#,
    )])
    .unwrap_err();
    assert!(matches!(
        error,
        LoadError::InvalidContextFloatProvider { .. }
    ));
}

#[test]
fn invalid_uniform_integer_range_aborts_the_execution_queue() {
    let mut vm = compile_int(
        &[
            (
                "example:overflow",
                "scoreboard objectives add state dummy\nscoreboard players set #after state 0\ncompute default integer {type:uniform,min:-2147483648,max:2147483647}\nscoreboard players set #after state 1\n",
            ),
            (
                "example:read_after",
                "return run scoreboard players get #after state\n",
            ),
        ],
        &[],
    );

    assert!(matches!(
        vm.execute_function("example:overflow", None, context(), LIMIT, drop).into_result(),
        Err(ExecutionError::ContextProviderEvaluationFailed { reason })
            if reason.contains("bound must be positive")
    ));
    assert_function(&mut vm, "example:read_after", returned(true, 0));
}

#[test]
fn macro_instantiation_uses_the_float_registry_and_typed_inline_parser() {
    let mut vm = compile_float(
        &[
            (
                "example:inline_macro",
                "$return run compute default integer {type:constant,value:$(value)}\n",
            ),
            (
                "example:named_macro",
                "$return run compute default float example:$(provider)\n",
            ),
            (
                "example:inline_call",
                "return run function example:inline_macro {value:-1}\n",
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
