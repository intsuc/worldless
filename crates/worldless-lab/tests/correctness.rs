#[test]
fn all_registered_suites_are_correct() {
    let report = worldless_lab::check(None).unwrap();
    assert!(!report.suites.is_empty());
    assert!(report.suites.iter().all(|suite| suite.invocation_count > 0));
}
