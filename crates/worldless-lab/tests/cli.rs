use std::process::{Command, Output};

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_worldless-lab"))
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn check_emits_machine_readable_success() {
    let output = run(&["check", "--suite", "concat", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["execution"]["vm_state"], "fresh");
    assert_eq!(report["execution"]["macro_cache"], "cold");
    assert_eq!(report["suites"][0]["suite"], "concat");
    assert_eq!(report["suites"][0]["case_count"], 5);
    assert_eq!(report["suites"][0]["variant_count"], 1);
    assert_eq!(report["suites"][0]["invocation_count"], 5);
}

#[test]
fn usage_errors_are_rejected_before_loading_a_suite() {
    for arguments in [
        &["check", "--suite"][..],
        &["check", "--format", "yaml"][..],
        &["check", "--samples", "1"][..],
        &["compare", "--samples", "0"][..],
    ] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty(), "{arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("usage: worldless-lab"),
            "{arguments:?}"
        );
    }
}

#[test]
fn unknown_suites_fail_without_success_output() {
    let output = run(&["check", "--suite", "missing", "--format", "text"]);
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown suite `missing`"));
}
