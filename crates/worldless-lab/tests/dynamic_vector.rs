use std::path::PathBuf;

use worldless::{
    CompiledProgram, CompoundTag, ExecutionContext, ExecutionOutcome, Pack, Position, Rotation, Vm,
};

const LIMIT: usize = 32_768;

fn program() -> CompiledProgram {
    let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packs/dynamic_vector");
    CompiledProgram::from_packs([Pack::directory(pack)]).unwrap()
}

fn context() -> ExecutionContext {
    ExecutionContext::new(Position::new(0.0, 0.0, 0.0), Rotation::new(0.0, 0.0))
}

fn invoke(vm: &mut Vm, function: &str) -> ExecutionOutcome {
    vm.execute_function(function, None, context(), LIMIT, drop)
        .into_result()
        .unwrap()
}

fn returned(success: bool, value: i32) -> ExecutionOutcome {
    ExecutionOutcome::Result { success, value }
}

#[test]
fn zero_seed_succeeds_for_every_layout() {
    let program = program();
    for variant in ["primitive_append", "preallocated", "chunked_16"] {
        let mut vm = program.create_vm(0);
        vm.set_storage(
            "worldless_lab:dynamic_vector/input",
            CompoundTag::from_snbt(r#"{length:1,seed:0,workload:"build"}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(
            invoke(
                &mut vm,
                &format!("worldless_lab:dynamic_vector/{variant}/run")
            ),
            returned(true, 1)
        );
        assert_eq!(
            vm.storage("worldless_lab:dynamic_vector/output").unwrap(),
            Some(&CompoundTag::from_snbt("{length:1,checksum:31}").unwrap())
        );
    }
}

#[test]
fn dynamic_access_distinguishes_missing_paths_from_zero_values() {
    let program = program();
    for (missing, existing) in [
        (
            r#"{work:{values:[I;],value:0,macro:{layout:"flat",index:0}}}"#,
            r#"{work:{values:[I;0],value:0,macro:{layout:"flat",index:0}}}"#,
        ),
        (
            r#"{work:{pages:[[I;]],value:0,macro:{layout:"chunked_16",page:0,offset:0}}}"#,
            r#"{work:{pages:[[I;0]],value:0,macro:{layout:"chunked_16",page:0,offset:0}}}"#,
        ),
    ] {
        let mut vm = program.create_vm(0);
        vm.set_storage(
            "worldless_lab:dynamic_vector/input",
            CompoundTag::from_snbt(r#"{length:0,seed:0,workload:"build"}"#).unwrap(),
        )
        .unwrap();
        assert_eq!(
            invoke(&mut vm, "worldless_lab:dynamic_vector/primitive_append/run"),
            returned(true, 1)
        );

        vm.set_storage(
            "dynamic_vector:state",
            CompoundTag::from_snbt(missing).unwrap(),
        )
        .unwrap();
        assert_eq!(
            invoke(&mut vm, "dynamic_vector:dispatch/read"),
            returned(false, 0)
        );
        assert_eq!(
            invoke(&mut vm, "dynamic_vector:dispatch/write"),
            returned(false, 0)
        );

        vm.set_storage(
            "dynamic_vector:state",
            CompoundTag::from_snbt(existing).unwrap(),
        )
        .unwrap();
        assert_eq!(
            invoke(&mut vm, "dynamic_vector:dispatch/read"),
            returned(true, 1)
        );
        assert_eq!(
            invoke(&mut vm, "dynamic_vector:dispatch/write"),
            returned(true, 1)
        );
    }
}
