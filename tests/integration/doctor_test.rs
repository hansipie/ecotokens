#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn doctor_json_reports_check_statuses() {
    let home = TempDir::new().unwrap();
    let config = TempDir::new().unwrap();

    let output = Command::new(ecotokens_bin())
        .arg("doctor")
        .arg("--json")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("XDG_CONFIG_HOME", config.path())
        .output()
        .expect("failed to run ecotokens doctor");

    assert!(
        output.status.success(),
        "doctor should succeed with warnings only, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).expect("doctor output is JSON");
    assert_eq!(json["ok"], true);
    let checks = json["checks"]
        .as_array()
        .expect("checks should be an array");
    assert!(
        checks.iter().any(|check| check["name"] == "config"
            && check["status"] == "warning"
            && check["message"]
                .as_str()
                .unwrap_or_default()
                .contains("defaults will be used")),
        "expected missing config warning, got: {json:#}"
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "metrics database"),
        "expected metrics database check, got: {json:#}"
    );
}
