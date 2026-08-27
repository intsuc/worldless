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
        let (namespace, path) = id.split_once(':').unwrap();
        let path = self
            .root
            .join("data")
            .join(namespace)
            .join("function")
            .join(format!("{path}.mcfunction"));
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

    let common = [
        "run".as_ref(),
        "--pack".as_ref(),
        low.root().as_os_str(),
        "--pack".as_ref(),
        high.root().as_os_str(),
        "--command-limit".as_ref(),
        "16".as_ref(),
    ];
    let output = worldless(&[
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        "example:returned".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "returned success=false value=0\n");
    assert!(output.stderr.is_empty());

    let output = worldless(&[
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        "example:value".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "returned success=true value=42\n");
    assert!(output.stderr.is_empty());

    let output = worldless(&[
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        "example:fell_through".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "fell-through\n");
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
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("limit of 0"), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");
}
