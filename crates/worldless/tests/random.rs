mod common;

use common::context;
use worldless::{
    ExecutionError, ExecutionOutcome, LoadError, MemoryResource, Pack, ResourceKind, Vm,
};

const LIMIT: usize = 128;

fn returned(value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result {
        success: true,
        value,
    }
}

fn load(
    functions: &[(&str, &str)],
    providers: &[(&str, &str)],
    world_seed: Option<i64>,
) -> Result<Vm, LoadError> {
    let functions = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    let providers = providers
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::NumberProvider, *id, *source));
    Vm::from_packs([Pack::memory(functions.chain(providers))], world_seed)
}

fn execute(vm: &mut Vm, id: &str) -> Result<ExecutionOutcome, ExecutionError> {
    vm.execute_function(id, None, context(), LIMIT)
}

#[test]
fn unnamed_values_share_the_vm_random_stream_and_ignore_the_world_seed() {
    let functions = [
        ("example:random", "return run random value 0..10\n"),
        (
            "example:compute",
            "return run compute default example:uniform integer\n",
        ),
    ];
    let providers = [("example:uniform", r#"{"type":"uniform","min":0,"max":10}"#)];

    for world_seed in [None, Some(-8_765_432_101)] {
        let mut vm = load(&functions, &providers, world_seed).unwrap();
        assert_eq!(execute(&mut vm, "example:random").unwrap(), returned(0));
        assert_eq!(execute(&mut vm, "example:compute").unwrap(), returned(6));
        assert_eq!(execute(&mut vm, "example:random").unwrap(), returned(8));
    }
}

#[test]
fn named_values_match_minecraft_and_persist_until_reset() {
    let mut vm = load(
        &[
            (
                "example:value",
                "return run random value 0..100 minecraft:test\n",
            ),
            ("example:reset", "return run random reset minecraft:test\n"),
            ("example:clear", "return run random reset *\n"),
        ],
        &[],
        Some(0),
    )
    .unwrap();

    for expected in [78, 81, 11, 9] {
        assert_eq!(
            execute(&mut vm, "example:value").unwrap(),
            returned(expected)
        );
    }
    assert_eq!(execute(&mut vm, "example:reset").unwrap(), returned(1));
    assert_eq!(execute(&mut vm, "example:value").unwrap(), returned(78));
    assert_eq!(execute(&mut vm, "example:clear").unwrap(), returned(1));
    assert_eq!(execute(&mut vm, "example:value").unwrap(), returned(78));
}

#[test]
fn explicit_seed_settings_match_negative_salt_and_identifier_vector() {
    let mut vm = load(
        &[
            (
                "example:reset",
                "return run random reset test:sequence -123456789 true true\n",
            ),
            (
                "example:value",
                "return run random value 0..100 test:sequence\n",
            ),
        ],
        &[],
        Some(1_234_567_890_123_456_789),
    )
    .unwrap();

    assert_eq!(execute(&mut vm, "example:reset").unwrap(), returned(1));
    for expected in [38, 71, 2, 26] {
        assert_eq!(
            execute(&mut vm, "example:value").unwrap(),
            returned(expected)
        );
    }
}

#[test]
fn missing_world_seed_is_explicit_and_failed_resets_are_atomic() {
    let mut vm = load(
        &[
            (
                "example:default_value",
                "return run random value 0..100 minecraft:test\n",
            ),
            (
                "example:explicit_reset",
                "return run random reset minecraft:test -1 false true\n",
            ),
            (
                "example:default_reset",
                "return run random reset minecraft:test\n",
            ),
            (
                "example:set_seed_required_default",
                "return run random reset * 7 true false\n",
            ),
            ("example:clear", "return run random reset *\n"),
        ],
        &[],
        None,
    )
    .unwrap();

    assert_eq!(
        execute(&mut vm, "example:default_value"),
        Err(ExecutionError::MissingWorldSeed {
            sequence: "minecraft:test".to_owned()
        })
    );
    assert_eq!(execute(&mut vm, "example:clear").unwrap(), returned(0));

    assert_eq!(
        execute(&mut vm, "example:explicit_reset").unwrap(),
        returned(1)
    );
    assert_eq!(
        execute(&mut vm, "example:default_value").unwrap(),
        returned(35)
    );
    assert_eq!(
        execute(&mut vm, "example:default_reset"),
        Err(ExecutionError::MissingWorldSeed {
            sequence: "minecraft:test".to_owned()
        })
    );
    assert_eq!(
        execute(&mut vm, "example:default_value").unwrap(),
        returned(20)
    );

    assert_eq!(
        execute(&mut vm, "example:set_seed_required_default").unwrap(),
        returned(1)
    );
    assert_eq!(
        execute(&mut vm, "example:default_value"),
        Err(ExecutionError::MissingWorldSeed {
            sequence: "minecraft:test".to_owned()
        })
    );
    assert_eq!(execute(&mut vm, "example:clear").unwrap(), returned(0));
}

#[test]
fn excluding_the_sequence_id_gives_equal_independent_streams() {
    let mut vm = load(
        &[
            (
                "example:reset_a",
                "return run random reset example:a 7 false false\n",
            ),
            (
                "example:reset_b",
                "return run random reset example:b 7 false false\n",
            ),
            (
                "example:value_a",
                "return run random value 0..100 example:a\n",
            ),
            (
                "example:value_b",
                "return run random value 0..100 example:b\n",
            ),
            (
                "example:default_value",
                "return run random value 0..100 example:default\n",
            ),
        ],
        &[],
        None,
    )
    .unwrap();

    execute(&mut vm, "example:reset_a").unwrap();
    execute(&mut vm, "example:reset_b").unwrap();
    for _ in 0..2 {
        let a = execute(&mut vm, "example:value_a").unwrap();
        let b = execute(&mut vm, "example:value_b").unwrap();
        assert_eq!(a, b);
    }
    assert!(matches!(
        execute(&mut vm, "example:default_value"),
        Err(ExecutionError::MissingWorldSeed { .. })
    ));
}

#[test]
fn invalid_runtime_ranges_do_not_report_or_consume_a_result() {
    let mut vm = load(
        &[
            (
                "example:setup",
                "scoreboard objectives add values dummy\nscoreboard players set #stored values 9\n",
            ),
            (
                "example:store",
                "execute store result score #stored values store success score #stored values run random value 5\nreturn run scoreboard players get #stored values\n",
            ),
            (
                "example:return_run",
                "return run random value -1073741824..1073741823\nreturn 12\n",
            ),
            (
                "example:widest_accepted",
                "return run random value -1073741823..1073741823\n",
            ),
            (
                "example:lazy_no_draw",
                "random value 5 minecraft:test\nreturn run random value 0..100 minecraft:test\n",
            ),
        ],
        &[],
        Some(0),
    )
    .unwrap();

    assert_eq!(
        execute(&mut vm, "example:setup").unwrap(),
        ExecutionOutcome::NoResult
    );
    assert_eq!(execute(&mut vm, "example:store").unwrap(), returned(9));
    assert_eq!(
        execute(&mut vm, "example:return_run").unwrap(),
        ExecutionOutcome::NoResult
    );
    assert_eq!(
        execute(&mut vm, "example:widest_accepted").unwrap(),
        returned(495_999_537)
    );
    assert_eq!(
        execute(&mut vm, "example:lazy_no_draw").unwrap(),
        returned(78)
    );

    let mut count_vm = load(
        &[(
            "example:lazy_count",
            "random value 5 example:counted\nreturn run random reset *\n",
        )],
        &[],
        Some(0),
    )
    .unwrap();
    assert_eq!(
        execute(&mut count_vm, "example:lazy_count").unwrap(),
        returned(1)
    );
}

#[test]
fn zero_results_are_successful_and_reset_all_reports_zero() {
    let mut vm = load(
        &[
            (
                "example:setup",
                "scoreboard objectives add values dummy\nexecute store result score #result values store success score #success values run random value -1..0\n",
            ),
            (
                "example:read_result",
                "return run scoreboard players get #result values\n",
            ),
            (
                "example:read_success",
                "return run scoreboard players get #success values\n",
            ),
            ("example:clear", "return run random reset *\n"),
        ],
        &[],
        None,
    )
    .unwrap();

    assert_eq!(execute(&mut vm, "example:clear").unwrap(), returned(0));
    execute(&mut vm, "example:setup").unwrap();
    assert_eq!(
        execute(&mut vm, "example:read_result").unwrap(),
        returned(0)
    );
    assert_eq!(
        execute(&mut vm, "example:read_success").unwrap(),
        returned(1)
    );
}

#[test]
fn roll_and_malformed_random_commands_are_rejected_during_loading() {
    for command in [
        "random roll 1..2",
        "random value ..",
        "random value 2..1",
        "random reset * 0 neither",
    ] {
        assert!(matches!(
            load(&[("example:invalid", command)], &[], None),
            Err(LoadError::InvalidFunction { .. })
        ));
    }
}
