use worldless_lab::ComparisonExecution;

#[test]
fn comparison_reports_fresh_and_persistent_measurement_contracts() {
    let fresh = worldless_lab::compare("concat", ComparisonExecution::Fresh, 1).unwrap();
    assert_eq!(fresh.execution.vm_state, "fresh");
    assert_eq!(fresh.execution.macro_cache, "cold");
    assert_eq!(fresh.warmup_discarded, 0);
    assert_eq!(fresh.measured_samples, 1);
    assert!(
        fresh
            .rows
            .iter()
            .all(|row| row.timing.durations_ns.len() == 1)
    );

    let persistent =
        worldless_lab::compare("concat", ComparisonExecution::Persistent { warmup: 1 }, 2).unwrap();
    assert_eq!(persistent.execution.vm_state, "persistent");
    assert_eq!(persistent.execution.macro_cache, "warm");
    assert_eq!(persistent.warmup_discarded, 1);
    assert_eq!(persistent.measured_samples, 2);
    assert!(
        persistent
            .rows
            .iter()
            .all(|row| row.timing.durations_ns.len() == 2)
    );
}

#[cfg(not(debug_assertions))]
#[test]
fn release_cli_reports_persistent_warm_measurements() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_worldless-lab"))
        .args([
            "compare",
            "--suite",
            "indirect_access",
            "--execution",
            "persistent",
            "--warmup",
            "1",
            "--samples",
            "2",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["execution"]["vm_state"], "persistent");
    assert_eq!(report["execution"]["macro_cache"], "warm");
    assert_eq!(report["warmup_discarded"], 1);
    assert_eq!(report["measured_samples"], 2);
    assert_eq!(report["rows"].as_array().unwrap().len(), 15);
}
