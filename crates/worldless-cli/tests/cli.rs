use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_PACK: AtomicU64 = AtomicU64::new(0);

struct TestPack {
    root: PathBuf,
}

impl TestPack {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "worldless cli test-{}-{}",
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

    fn write_function(&self, id: &str, source: &str) {
        self.write_resource("function", id, "mcfunction", source);
    }

    fn write_predicate(&self, id: &str, source: &str) {
        self.write_resource("predicate", id, "json", source);
    }

    fn write_resource(&self, kind: &str, id: &str, extension: &str, source: &str) {
        let (namespace, path) = id.split_once(':').unwrap();
        let path = self
            .root
            .join("data")
            .join(namespace)
            .join(kind)
            .join(format!("{path}.{extension}"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
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

fn worldless(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_worldless"))
        .args(arguments)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn check_loads_an_explicit_pack_stack() {
    let pack = TestPack::new();
    let output = worldless(&["check".as_ref(), "--pack".as_ref(), pack.root().as_os_str()]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "ok\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn run_reports_each_function_outcome_and_uses_later_packs_as_higher_priority() {
    let low = TestPack::new();
    low.write_function("example:returned", "return 1\n");
    let high = TestPack::new();
    high.write_function("example:returned", "return fail\n");
    high.write_function("example:value", "return 42\n");
    high.write_function("example:fell_through", "");
    high.write_function(
        "example:context",
        "return run execute positioned ^ ^ ^1 if predicate example:ahead\n",
    );
    high.write_predicate(
        "example:ahead",
        r#"{"type":"location_check","predicate":{"position":{"x":{"min":-0.001,"max":0.001},"y":64,"z":{"min":0.999,"max":1.001}}}}"#,
    );

    let common: Vec<&std::ffi::OsStr> = vec![
        "run".as_ref(),
        "--pack".as_ref(),
        low.root().as_os_str(),
        "--pack".as_ref(),
        high.root().as_os_str(),
        "--command-limit".as_ref(),
        "16".as_ref(),
        "--position".as_ref(),
        "0".as_ref(),
        "64".as_ref(),
        "0".as_ref(),
        "--rotation".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
    ];
    let run = |function: &str| {
        let mut arguments = common.clone();
        arguments.push(function.as_ref());
        worldless(&arguments)
    };

    let output = run("example:returned");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "returned success=false value=0\n");
    assert!(output.stderr.is_empty());

    let output = run("example:value");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "returned success=true value=42\n");
    assert!(output.stderr.is_empty());

    let output = run("example:fell_through");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "fell-through\n");
    assert!(output.stderr.is_empty());

    let output = run("example:context");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "returned success=true value=1\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn usage_load_and_execution_failures_have_distinct_exit_codes() {
    let missing = std::env::temp_dir().join(format!(
        "worldless-cli-missing-{}-{}",
        std::process::id(),
        NEXT_PACK.fetch_add(1, Ordering::Relaxed)
    ));
    let output = worldless(&[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        missing.as_os_str(),
        "--command-limit".as_ref(),
        "invalid".as_ref(),
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless(&["check".as_ref(), "--pack".as_ref(), missing.as_os_str()]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");

    let pack = TestPack::new();
    pack.write_function("example:main", "return 1\n");
    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--command-limit".as_ref(),
        "0".as_ref(),
        "--position".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "--rotation".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("limit of 0"), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");
}
