use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use worldless::{ExecutionError, FunctionOutcome, LoadError, Vm};

static NEXT_PACK: AtomicU64 = AtomicU64::new(0);

struct TestPack {
    root: PathBuf,
}

impl TestPack {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless-test-{}-{}",
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

    fn write_function(&self, id: &str, contents: &str) {
        let (namespace, path) = id.split_once(':').unwrap();
        let path = self
            .root
            .join("data")
            .join(namespace)
            .join("function")
            .join(format!("{path}.mcfunction"));
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

#[test]
fn executes_nested_paths_and_return_run() {
    let pack = TestPack::new();
    pack.write_function(
        "example:main",
        "function example:child\nreturn run fun\\\nction example:math/answer\nreturn 99\n",
    );
    pack.write_function("example:child", "return 7\n");
    pack.write_function("example:math/answer", "return 42\n");

    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert_eq!(
        vm.execute_function("example:main", 8).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 42
        }
    );
}

#[test]
fn a_normal_child_return_does_not_return_from_its_parent() {
    let pack = TestPack::new();
    pack.write_function("example:main", "function example:child\nreturn 5\n");
    pack.write_function("example:child", "return 99\n");

    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert_eq!(
        vm.execute_function("example:main", 5).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 5
        }
    );
}

#[test]
fn reports_failure_and_fallthrough_distinctly() {
    let pack = TestPack::new();
    pack.write_function("example:failure", "return fail\n");
    pack.write_function("example:empty", "# nothing to execute\n");
    pack.write_function("example:", "return 8\n");

    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert_eq!(
        vm.execute_function("example:failure", 2).unwrap(),
        FunctionOutcome::Returned {
            success: false,
            value: 0
        }
    );
    assert_eq!(
        vm.execute_function("example:empty", 2).unwrap(),
        FunctionOutcome::FellThrough
    );
    assert_eq!(
        vm.execute_function("example:", 2).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 8
        }
    );
}

#[test]
fn return_run_converts_child_fallthrough_to_failure() {
    let pack = TestPack::new();
    pack.write_function("example:main", "return run function example:target\n");
    pack.write_function("example:target", "function example:child\n");
    pack.write_function("example:child", "return 9\n");

    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert_eq!(
        vm.execute_function("example:main", 4).unwrap(),
        FunctionOutcome::Returned {
            success: false,
            value: 0
        }
    );
}

#[test]
fn enforces_the_minecraft_queue_limit_without_rust_recursion() {
    let pack = TestPack::new();
    pack.write_function("example:loop", "function example:loop\n");
    let mut vm = Vm::load_directory(pack.root()).unwrap();

    assert_eq!(
        vm.execute_function("example:loop", 10),
        Err(ExecutionError::CommandLimitExceeded { limit: 10 })
    );
}

#[test]
fn reaching_the_limit_before_the_first_command_is_an_error() {
    let pack = TestPack::new();
    pack.write_function("example:main", "return 1\n");
    let mut vm = Vm::load_directory(pack.root()).unwrap();

    assert_eq!(
        vm.execute_function("example:main", 1),
        Err(ExecutionError::CommandLimitExceeded { limit: 1 })
    );
    assert_eq!(
        vm.execute_function("example:main", 2).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 1
        }
    );
}

#[test]
fn unresolved_nested_calls_fail_without_stopping_the_function() {
    let pack = TestPack::new();
    pack.write_function("example:main", "function example:missing\nreturn 6\n");
    let mut vm = Vm::load_directory(pack.root()).unwrap();

    assert_eq!(
        vm.execute_function("example:main", 3).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 6
        }
    );

    pack.write_function("example:only_missing", "function example:missing\n");
    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert_eq!(
        vm.execute_function("example:only_missing", 3).unwrap(),
        FunctionOutcome::FellThrough
    );
}

#[test]
fn unresolved_return_run_discards_the_current_frame() {
    let pack = TestPack::new();
    pack.write_function(
        "example:main",
        "return run function example:missing\nreturn 6\n",
    );
    let mut vm = Vm::load_directory(pack.root()).unwrap();

    assert_eq!(
        vm.execute_function("example:main", 3).unwrap(),
        FunctionOutcome::FellThrough
    );
}

#[test]
fn requires_the_target_minor_aware_pack_format() {
    let pack = TestPack::new();
    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","pack_format":118}}"#,
    )
    .unwrap();
    assert!(matches!(
        Vm::load_directory(pack.root()),
        Err(LoadError::InvalidPack { .. })
    ));

    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","min_format":[118,1],"max_format":[118,2]}}"#,
    )
    .unwrap();
    assert!(matches!(
        Vm::load_directory(pack.root()),
        Err(LoadError::InvalidPack { .. })
    ));
}

#[test]
fn accepts_official_compatible_format_encodings() {
    let pack = TestPack::new();
    pack.write_function("example:main", "return 1\n");
    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","pack_format":81,"supported_formats":[81,81],"min_format":[81,0],"max_format":[118,0]}}"#,
    )
    .unwrap();
    assert!(Vm::load_directory(pack.root()).is_ok());

    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","pack_format":null,"supported_formats":null,"min_format":118.0,"max_format":4294967414}}"#,
    )
    .unwrap();
    assert!(Vm::load_directory(pack.root()).is_ok());
}

#[test]
fn rejects_unsupported_pack_and_function_features() {
    let pack = TestPack::new();
    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","min_format":118,"max_format":118},"overlays":{"entries":[]}}"#,
    )
    .unwrap();
    assert!(matches!(
        Vm::load_directory(pack.root()),
        Err(LoadError::UnsupportedPack { .. })
    ));

    fs::write(
        pack.root().join("pack.mcmeta"),
        r#"{"pack":{"description":"test","min_format":118,"max_format":118}}"#,
    )
    .unwrap();
    pack.write_function("example:macro", "$return $(value)\n");
    assert!(matches!(
        Vm::load_directory(pack.root()),
        Err(LoadError::InvalidFunction { .. })
    ));
}

#[test]
fn ignores_old_plural_and_invalid_resource_paths() {
    let pack = TestPack::new();
    let plural = pack
        .root()
        .join("data/example/functions/not_loaded.mcfunction");
    fs::create_dir_all(plural.parent().unwrap()).unwrap();
    fs::write(plural, "return 1\n").unwrap();
    pack.write_function("example:valid", "return 2\n");
    let invalid = pack.root().join("data/example/function/Upper.mcfunction");
    fs::write(invalid, "not a supported command\n").unwrap();

    let mut vm = Vm::load_directory(pack.root()).unwrap();
    assert!(matches!(
        vm.execute_function("example:not_loaded", 2),
        Err(ExecutionError::UnknownFunction { .. })
    ));
    assert_eq!(
        vm.execute_function("example:valid", 2).unwrap(),
        FunctionOutcome::Returned {
            success: true,
            value: 2
        }
    );
}

#[cfg(any(unix, windows))]
#[test]
fn rejects_symbolic_links_before_reading_pack_resources() {
    let pack = TestPack::new();
    let target = pack.root().join("linked-target.mcfunction");
    fs::write(&target, "return 1\n").unwrap();
    let link = pack.root().join("data/example/function/linked.mcfunction");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    if let Err(error) = create_file_symlink(&target, &link) {
        #[cfg(windows)]
        if error.kind() == std::io::ErrorKind::PermissionDenied
            || error.raw_os_error() == Some(1314)
        {
            return;
        }
        panic!("failed to create test symlink: {error}");
    }

    assert!(matches!(
        Vm::load_directory(pack.root()),
        Err(LoadError::UnsupportedPack {
            feature: "symbolic links",
            ..
        })
    ));
}

#[cfg(unix)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_file_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}
