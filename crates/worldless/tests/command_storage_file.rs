mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use common::context;
use worldless::{
    CompiledProgram, CompoundTag, ExecutionOutcome, MemoryResource, Pack, ResourceKind,
};

static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

struct TestFile {
    root: PathBuf,
    path: PathBuf,
}

impl TestFile {
    fn new(contents: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-command-storage-test-{}-{}",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("command_storage.dat");
        fs::write(&path, contents).unwrap();
        Self { root, path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            eprintln!("failed to remove {}: {error}", self.root.display());
        }
    }
}

fn push_modified_utf8(output: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    output.extend_from_slice(&u16::try_from(bytes.len()).unwrap().to_be_bytes());
    output.extend_from_slice(bytes);
}

fn push_named_header(output: &mut Vec<u8>, tag_type: u8, name: &str) {
    output.push(tag_type);
    push_modified_utf8(output, name);
}

fn storage_file(version: i32, values: &[(&str, i32)]) -> Vec<u8> {
    let mut output = Vec::new();
    push_named_header(&mut output, 10, "");
    push_named_header(&mut output, 10, "data");
    push_named_header(&mut output, 10, "contents");
    for &(path, value) in values {
        push_named_header(&mut output, 10, path);
        push_named_header(&mut output, 3, "value");
        output.extend_from_slice(&value.to_be_bytes());
        output.push(0);
    }
    output.push(0);
    output.push(0);
    push_named_header(&mut output, 3, "DataVersion");
    output.extend_from_slice(&version.to_be_bytes());
    output.push(0);
    output
}

fn empty_program() -> CompiledProgram {
    CompiledProgram::from_packs([Pack::memory(std::iter::empty::<MemoryResource>())]).unwrap()
}

#[test]
fn loads_files_before_execution_and_keeps_vm_state_independent() {
    let program = CompiledProgram::from_packs([Pack::memory([MemoryResource::new(
        ResourceKind::Function,
        "example:read",
        "return run data get storage probe:state value\n",
    )])])
    .unwrap();
    let file = TestFile::new(&storage_file(5017, &[("state", 7), ("", 9)]));
    let mut loaded = program.create_vm(0);
    let untouched = program.create_vm(0);

    loaded
        .load_command_storage_files([("probe", file.path())])
        .unwrap();

    assert_eq!(
        loaded.storage("probe:state").unwrap(),
        Some(&CompoundTag::from_snbt("{value:7}").unwrap())
    );
    assert_eq!(
        loaded.storage("probe:").unwrap(),
        Some(&CompoundTag::from_snbt("{value:9}").unwrap())
    );
    assert_eq!(untouched.storage("probe:state").unwrap(), None);
    assert_eq!(
        loaded
            .execute_function("example:read", None, context(), 3, drop)
            .into_result(),
        Ok(ExecutionOutcome::Result {
            success: true,
            value: 7,
        })
    );
}

#[test]
fn loaded_storage_is_visible_to_macros_during_the_first_load_tick() {
    let program = CompiledProgram::from_packs([Pack::memory([
        MemoryResource::new(
            ResourceKind::Function,
            "example:load",
            "scoreboard objectives add state dummy\nfunction example:macro with storage probe:args\n",
        ),
        MemoryResource::new(
            ResourceKind::Function,
            "example:macro",
            "$scoreboard players set #loaded state $(value)\n",
        ),
        MemoryResource::new(
            ResourceKind::FunctionTag,
            "minecraft:load",
            r#"{"values":["example:load"]}"#,
        ),
    ])])
    .unwrap();
    let file = TestFile::new(&storage_file(5017, &[("args", 7)]));
    let mut vm = program.create_vm(0);

    vm.load_command_storage_files([("probe", file.path())])
        .unwrap();
    assert!(vm.tick(context(), 8).failures().is_empty());

    assert_eq!(
        vm.execute_command("scoreboard players get #loaded state", context(), 2, drop,)
            .into_result(),
        Ok(ExecutionOutcome::Result {
            success: true,
            value: 7,
        })
    );
}

#[test]
fn validates_every_file_before_replacing_any_namespace() {
    let valid = TestFile::new(&storage_file(5017, &[("new", 7)]));
    let wrong_version = TestFile::new(&storage_file(5016, &[("ignored", 8)]));
    let empty = TestFile::new(&storage_file(5017, &[]));
    let mut vm = empty_program().create_vm(0);
    vm.set_storage("probe:old", CompoundTag::from_snbt("{value:1}").unwrap())
        .unwrap();
    vm.set_storage("keep:old", CompoundTag::from_snbt("{value:2}").unwrap())
        .unwrap();

    assert!(
        vm.load_command_storage_files([("probe", valid.path()), ("other", wrong_version.path()),])
            .is_err()
    );
    assert!(vm.storage("probe:old").unwrap().is_some());
    assert_eq!(vm.storage("probe:new").unwrap(), None);
    assert!(vm.storage("keep:old").unwrap().is_some());

    vm.load_command_storage_files([("probe", valid.path())])
        .unwrap();
    assert_eq!(vm.storage("probe:old").unwrap(), None);
    assert_eq!(
        vm.storage("probe:new").unwrap(),
        Some(&CompoundTag::from_snbt("{value:7}").unwrap())
    );
    assert!(vm.storage("keep:old").unwrap().is_some());

    vm.load_command_storage_files([("probe", empty.path())])
        .unwrap();
    assert_eq!(vm.storage("probe:new").unwrap(), None);
    assert!(vm.storage("keep:old").unwrap().is_some());
}

#[test]
fn rejects_invalid_and_duplicate_namespaces_before_file_access() {
    let missing = std::env::temp_dir().join(format!(
        "worldless-command-storage-missing-{}-{}",
        std::process::id(),
        NEXT_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut vm = empty_program().create_vm(0);

    let invalid = vm
        .load_command_storage_files([("Upper", &missing)])
        .unwrap_err();
    assert!(
        invalid
            .to_string()
            .contains("invalid command-storage namespace")
    );

    let duplicate = vm
        .load_command_storage_files([("probe", &missing), ("probe", &missing)])
        .unwrap_err();
    assert!(
        duplicate
            .to_string()
            .contains("duplicate command-storage namespace")
    );
}
