mod common;

use common::context;
use worldless::{
    CompiledProgram, ExecutionError, ExecutionOutcome, LoadError, MemoryResource, Pack,
    ResourceKind, Vm,
};

const LIMIT: usize = 128;

fn returned(value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result {
        success: true,
        value,
    }
}

fn load(seed: i64, functions: &[(&str, &str)]) -> Result<Vm, LoadError> {
    let resources = functions
        .iter()
        .map(|(id, source)| MemoryResource::new(ResourceKind::Function, *id, *source));
    CompiledProgram::from_packs([Pack::memory(resources)]).map(|program| program.create_vm(seed))
}

#[test]
fn seed_returns_the_java_narrowing_conversion_of_the_configured_seed() {
    for seed in [
        i64::MIN,
        -1,
        0,
        1,
        0x0000_0000_8000_0000,
        0x0123_4567_89ab_cdef,
        i64::MAX,
    ] {
        let mut vm = load(seed, &[]).unwrap();
        assert_eq!(
            vm.execute_command("seed", context(), LIMIT, drop)
                .into_result()
                .unwrap(),
            returned(seed as i32),
            "seed {seed}"
        );
    }
}

#[test]
fn seed_participates_in_return_and_store_result_flow() {
    let seed = 0x0123_4567_0000_0000;
    let mut vm = load(
        seed,
        &[
            ("example:return_seed", "return run seed\nreturn 7\n"),
            (
                "example:store_seed",
                "scoreboard objectives add values dummy\nexecute store result score #result values store success score #success values run seed\n",
            ),
            (
                "example:read_result",
                "return run scoreboard players get #result values\n",
            ),
            (
                "example:read_success",
                "return run scoreboard players get #success values\n",
            ),
        ],
    )
    .unwrap();

    assert_eq!(
        vm.execute_function("example:return_seed", None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        returned(seed as i32)
    );
    assert_eq!(
        vm.execute_function("example:store_seed", None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        ExecutionOutcome::NoResult
    );
    assert_eq!(
        vm.execute_function("example:read_result", None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        returned(seed as i32)
    );
    assert_eq!(
        vm.execute_function("example:read_success", None, context(), LIMIT, drop)
            .into_result()
            .unwrap(),
        returned(1)
    );
}

#[test]
fn seed_is_counted_as_an_ordinary_command() {
    let mut vm = load(42, &[]).unwrap();

    assert_eq!(
        vm.execute_command("seed", context(), 1, drop).into_result(),
        Err(ExecutionError::CommandLimitExceeded { limit: 1 })
    );
    assert_eq!(
        vm.execute_command("seed", context(), 2, drop)
            .into_result()
            .unwrap(),
        returned(42)
    );
}

#[test]
fn observing_the_seed_does_not_consume_random_state() {
    let mut observed = load(-8_765_432_101, &[]).unwrap();
    let mut control = load(-8_765_432_101, &[]).unwrap();

    observed
        .execute_command("seed", context(), LIMIT, drop)
        .into_result()
        .unwrap();
    for command in [
        "random value 0..100",
        "random value 0..100 example:sequence",
        "random value 0..100",
        "random value 0..100 example:sequence",
    ] {
        assert_eq!(
            observed
                .execute_command(command, context(), LIMIT, drop)
                .into_result()
                .unwrap(),
            control
                .execute_command(command, context(), LIMIT, drop)
                .into_result()
                .unwrap()
        );
    }
}

#[test]
fn seed_accepts_no_arguments() {
    for source in ["seed extra\n", "seed 0\n"] {
        assert!(matches!(
            load(0, &[("example:invalid", source)]),
            Err(LoadError::InvalidFunction { .. })
        ));
    }
}
