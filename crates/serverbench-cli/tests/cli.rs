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
