use std::process::Command;
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_serverbench"))
}
#[test]
fn help_and_invalid_parameters() {
    assert!(cli().arg("--help").output().unwrap().status.success());
    assert_eq!(
        cli()
            .args(["benchmark", "--quick", "--full"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        cli()
            .args(["benchmark", "--monthly-cost", "-1"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(2)
    );
}
#[cfg(target_os = "linux")]
#[test]
fn info_is_machine_readable_and_anonymous() {
    let output = cli().args(["info", "--json"]).output().unwrap();
    assert!(output.status.success());
    let v: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(v["cpu"]["usable_cpus"].as_u64().unwrap() > 0);
    assert!(v.get("hostname").is_none());
    assert!(v.get("machine_id").is_none());
}
#[test]
fn missing_result_fails() {
    assert_eq!(
        cli()
            .args(["compare", "nonexistent-a.json", "nonexistent-b.json"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn repeat_options_are_checked_before_starting_work() {
    for args in [
        vec!["benchmark", "--repeat", "0"],
        vec!["benchmark", "--repeat", "101"],
        vec!["benchmark", "--repeat", "1", "--repeat-pause", "1s"],
        vec!["benchmark", "--continue-on-error"],
        vec!["benchmark", "--repeat", "2", "--repeat-pause", "0s"],
    ] {
        let result = cli().args(&args).output().unwrap();
        assert_eq!(result.status.code(), Some(2), "{args:?}");
    }
    let help = cli().args(["benchmark", "--help"]).output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("--repeat"));
    assert!(help.contains("--continue-on-error"));
    let version = cli().arg("--version").output().unwrap();
    assert!(String::from_utf8(version.stdout)
        .unwrap()
        .contains(env!("CARGO_PKG_VERSION")));
}
#[test]
fn conditional_comparison_on_single_runs_reports_insufficient_repeats() {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/results");
    let output = cli()
        .arg("compare")
        .arg(root.join("legacy-v1-stable.json"))
        .arg(root.join("legacy-v1-variable.json"))
        .args(["--uncertainty", "independent-runs"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("insufficient_repeats"));
    assert!(text.contains("cannot verify these assumptions"));
}
