use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
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

    fn write_function_tag(&self, id: &str, source: &str) {
        self.write_resource("tags/function", id, "json", source);
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

fn worldless_with_stdin(arguments: &[&std::ffi::OsStr], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_worldless"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn check_accepts_empty_and_explicit_pack_stacks() {
    let output = worldless(&["check".as_ref()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "ok\n");
    assert!(output.stderr.is_empty());

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
    high.write_function(
        "example:default_context",
        "return run execute positioned ^ ^ ^1 if predicate example:default_ahead\n",
    );
    high.write_predicate(
        "example:ahead",
        r#"{"type":"location_check","predicate":{"position":{"x":{"min":-0.001,"max":0.001},"y":64,"z":{"min":0.999,"max":1.001}}}}"#,
    );
    high.write_predicate(
        "example:default_ahead",
        r#"{"type":"location_check","predicate":{"position":{"x":{"min":-0.001,"max":0.001},"y":0,"z":{"min":0.999,"max":1.001}}}}"#,
    );

    let common: Vec<&std::ffi::OsStr> = vec![
        "run".as_ref(),
        "--pack".as_ref(),
        low.root().as_os_str(),
        "--pack".as_ref(),
        high.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
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
        arguments.push("function".as_ref());
        arguments.push(function.as_ref());
        worldless(&arguments)
    };

    let output = run("example:returned");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:returned\"\n",
            "feedback kind=success text=\"Function example:returned returned 0\"\n",
            "result success=false value=0\n",
        )
    );
    assert!(output.stderr.is_empty());

    let output = run("example:value");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:value\"\n",
            "feedback kind=success text=\"Function example:value returned 42\"\n",
            "result success=true value=42\n",
        )
    );
    assert!(output.stderr.is_empty());

    let output = run("example:fell_through");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        "feedback kind=success text=\"Running function example:fell_through\"\nno-result\n"
    );
    assert!(output.stderr.is_empty());

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        low.root().as_os_str(),
        "--pack".as_ref(),
        high.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "function".as_ref(),
        "example:default_context".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:default_context\"\n",
            "feedback kind=success text=\"Function example:default_context returned 1\"\n",
            "result success=true value=1\n",
        )
    );
    assert!(output.stderr.is_empty());

    let output = run("example:context");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:context\"\n",
            "feedback kind=success text=\"Function example:context returned 1\"\n",
            "result success=true value=1\n",
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn run_executes_macro_arguments_function_tags_and_one_raw_command() {
    let pack = TestPack::new();
    pack.write_function("example:macro", "$return $(value)\n");
    pack.write_function("example:one", "return 1\n");
    pack.write_function("example:two", "return 2\n");
    pack.write_function_tag(
        "example:both",
        r#"{"values":["example:one","example:two"]}"#,
    );

    let run = |target: &[&std::ffi::OsStr]| {
        let mut arguments = vec![
            "run".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ];
        arguments.extend_from_slice(target);
        worldless(&arguments)
    };

    let output = run(&[
        "function".as_ref(),
        "--arguments".as_ref(),
        "{value:9}".as_ref(),
        "example:macro".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:macro\"\n",
            "feedback kind=success text=\"Function example:macro returned 9\"\n",
            "result success=true value=9\n",
        )
    );
    assert!(output.stderr.is_empty());

    let output = run(&["tag".as_ref(), "example:both".as_ref()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running functions example:one, example:two\"\n",
            "feedback kind=success text=\"Function example:one returned 1\"\n",
            "feedback kind=success text=\"Function example:two returned 2\"\n",
            "result success=true value=3\n",
        )
    );
    assert!(output.stderr.is_empty());

    let output = run(&["command".as_ref(), "return 7".as_ref()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&output.stdout), "result success=true value=7\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn run_renders_feedback_before_the_command_outcome() {
    let output = worldless(&[
        "run".as_ref(),
        "--world-seed".as_ref(),
        "42".as_ref(),
        "command".as_ref(),
        "seed".as_ref(),
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        "feedback kind=success text=\"Seed: [42]\"\nresult success=true value=42\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn check_is_seedless_and_run_requires_a_seed_for_named_random_sequences() {
    let pack = TestPack::new();
    pack.write_function(
        "example:random",
        "return run random value 0..100 minecraft:test\n",
    );

    let output = worldless(&["check".as_ref(), "--pack".as_ref(), pack.root().as_os_str()]);
    assert_eq!(output.status.code(), Some(0));

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "function".as_ref(),
        "example:random".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        text(&output.stderr).contains("missing required --world-seed"),
        "{}",
        text(&output.stderr)
    );
    assert!(text(&output.stderr).contains("usage: worldless"));

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "function".as_ref(),
        "example:random".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:random\"\n",
            "feedback kind=success text=\"Function example:random returned 78\"\n",
            "result success=true value=78\n",
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn repl_without_packs_keeps_vm_state_and_resets_each_invocation_budget() {
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--world-seed".as_ref(),
            "0".as_ref(),
            "--command-limit".as_ref(),
            "2".as_ref(),
        ],
        b"scoreboard objectives add values dummy\nscoreboard players set #counter values 1\nscoreboard players add #counter values 2\nscoreboard players get #counter values\n:quit\nreturn 99\n",
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        text(&output.stdout),
        text(&output.stderr)
    );
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Created new objective [values]\"\n",
            "result success=true value=1\n",
            "feedback kind=success text=\"Set [values] for #counter to 1\"\n",
            "result success=true value=1\n",
            "feedback kind=success text=\"Added 2 to [values] for #counter (now 3)\"\n",
            "result success=true value=3\n",
            "feedback kind=success text=\"#counter has 3 [values]\"\n",
            "result success=true value=3\n",
        )
    );
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
}

#[test]
fn repl_writes_success_and_failure_feedback_to_stdout_in_execution_order() {
    let pack = TestPack::new();
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"scoreboard objectives add values dummy\nscoreboard objectives add values dummy\n:quit\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Created new objective [values]\"\n",
            "result success=true value=1\n",
            "feedback kind=failure text=\"An objective already exists by that name\"\n",
            "result success=false value=0\n",
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn repl_accepts_raw_macro_function_and_function_tag_commands() {
    let pack = TestPack::new();
    pack.write_function("example:macro", "$return $(value)\n");
    pack.write_function("example:one", "return 1\n");
    pack.write_function("example:two", "return 2\n");
    pack.write_function_tag(
        "example:both",
        r#"{"values":["example:one","example:two"]}"#,
    );
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"function example:macro {value:9}\nfunction #example:both\n:quit\n",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "feedback kind=success text=\"Running function example:macro\"\n",
            "feedback kind=success text=\"Function example:macro returned 9\"\n",
            "result success=true value=9\n",
            "feedback kind=success text=\"Running functions example:one, example:two\"\n",
            "feedback kind=success text=\"Function example:one returned 1\"\n",
            "feedback kind=success text=\"Function example:two returned 2\"\n",
            "result success=true value=3\n",
        )
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn repl_ignores_empty_lines_resets_context_and_stops_at_eof() {
    let pack = TestPack::new();
    pack.write_predicate(
        "example:origin",
        r#"{"type":"location_check","predicate":{"position":{"x":0,"y":0,"z":0}}}"#,
    );
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"\r\nexecute positioned 1 2 3 run return 1\nreturn run execute if predicate example:origin",
    );

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        text(&output.stdout),
        concat!(
            "result success=true value=1\n",
            "feedback kind=success text=\"Test passed\"\n",
            "result success=true value=1\n",
        )
    );
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
}

#[test]
fn repl_continues_after_execution_errors_and_reports_them_at_exit() {
    let pack = TestPack::new();
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"\n\r\nnot-a-worldless-command\nreturn 7\n",
    );

    assert_eq!(output.status.code(), Some(4));
    assert_eq!(text(&output.stdout), "result success=true value=7\n");
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: line 3: "), "{stderr}");
    assert!(stderr.contains("command compilation failed"), "{stderr}");
    assert!(!stderr.contains("worldless> "), "{stderr}");
}

#[test]
fn repl_rejects_non_utf8_input_as_an_execution_failure() {
    let pack = TestPack::new();
    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            pack.root().as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        &[0xff],
    );

    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("failed to read REPL input"), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");
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
        "--world-seed".as_ref(),
        "0".as_ref(),
        "--command-limit".as_ref(),
        "invalid".as_ref(),
        "function".as_ref(),
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

    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--pack".as_ref(),
            missing.as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");

    let pack = TestPack::new();
    pack.write_function("example:main", "return 1\n");

    let output = worldless_with_stdin(
        &["repl".as_ref(), "--pack".as_ref(), pack.root().as_os_str()],
        b"",
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.contains("missing required --world-seed"), "{stderr}");
    assert!(stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(text(&output.stderr).contains("usage: worldless"));

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "function".as_ref(),
        "--arguments".as_ref(),
        "not-a-compound".as_ref(),
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("error: invalid function arguments:"),
        "{stderr}"
    );
    assert!(!stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "command".as_ref(),
        "--not-a-command".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless(&[
        "run".as_ref(),
        "--pack".as_ref(),
        pack.root().as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "--command-limit".as_ref(),
        "0".as_ref(),
        "--position".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "--rotation".as_ref(),
        "0".as_ref(),
        "0".as_ref(),
        "function".as_ref(),
        "example:main".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(stderr.contains("limit of 0"), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");
}

#[test]
fn command_storage_load_failures_use_the_load_exit_code_for_run_and_repl() {
    let missing = std::env::temp_dir().join(format!(
        "worldless-cli-missing-storage-{}-{}",
        std::process::id(),
        NEXT_PACK.fetch_add(1, Ordering::Relaxed)
    ));

    let output = worldless(&[
        "run".as_ref(),
        "--command-storage".as_ref(),
        "probe".as_ref(),
        missing.as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "command".as_ref(),
        "seed".as_ref(),
    ]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");

    let output = worldless_with_stdin(
        &[
            "repl".as_ref(),
            "--command-storage".as_ref(),
            "probe".as_ref(),
            missing.as_os_str(),
            "--world-seed".as_ref(),
            "0".as_ref(),
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = text(&output.stderr);
    assert!(stderr.starts_with("error: "), "{stderr}");
    assert!(!stderr.contains("usage: worldless"), "{stderr}");
}

#[test]
fn run_loads_command_storage_before_executing_the_command() {
    let directory = TestPack::new();
    let storage = directory.root().join("probe.dat");
    fs::write(
        &storage,
        [
            0x0a, 0x00, 0x00, 0x03, 0x00, 0x0b, b'D', b'a', b't', b'a', b'V', b'e', b'r', b's',
            b'i', b'o', b'n', 0x00, 0x00, 0x13, 0x97, 0x0a, 0x00, 0x04, b'd', b'a', b't', b'a',
            0x0a, 0x00, 0x08, b'c', b'o', b'n', b't', b'e', b'n', b't', b's', 0x0a, 0x00, 0x05,
            b's', b't', b'a', b't', b'e', 0x03, 0x00, 0x05, b'v', b'a', b'l', b'u', b'e', 0x00,
            0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00,
        ],
    )
    .unwrap();

    let output = worldless(&[
        "run".as_ref(),
        "--command-storage".as_ref(),
        "probe".as_ref(),
        storage.as_os_str(),
        "--world-seed".as_ref(),
        "0".as_ref(),
        "command".as_ref(),
        "data get storage probe:state value".as_ref(),
    ]);

    assert_eq!(output.status.code(), Some(0));
    assert!(
        text(&output.stdout).ends_with("result success=true value=7\n"),
        "{}",
        text(&output.stdout)
    );
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
}
