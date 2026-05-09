use serde_json::Value;
use std::process::Command;
use tempfile::tempdir;

mod helpers;
use helpers::ecotokens;

#[test]
fn test_config_debug_toggle() {
    let tmp = tempdir().expect("failed to create temp dir");
    let config_home = tmp.path();

    // 1. Check initial state (default should be false)
    let output = Command::new(ecotokens())
        .arg("config")
        .arg("--json")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens config");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("failed to parse JSON output");
    assert_eq!(v["debug"], false);

    // 2. Enable debug
    let output = Command::new(ecotokens())
        .arg("config")
        .arg("--debug")
        .arg("true")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens config --debug true");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("settings updated"));

    // 3. Verify it is enabled
    let output = Command::new(ecotokens())
        .arg("config")
        .arg("--json")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens config");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("failed to parse JSON output");
    assert_eq!(v["debug"], true);

    // 4. Disable debug
    let output = Command::new(ecotokens())
        .arg("config")
        .arg("--debug")
        .arg("false")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens config --debug false");

    assert!(output.status.success());

    // 5. Verify it is disabled
    let output = Command::new(ecotokens())
        .arg("config")
        .arg("--json")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens config");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: Value = serde_json::from_str(&stdout).expect("failed to parse JSON output");
    assert_eq!(v["debug"], false);
}

#[test]
fn test_watch_no_log_when_debug_disabled() {
    let tmp = tempdir().expect("failed to create temp dir");
    let config_home = tmp.path();
    let watch_dir = tmp.path().join("watch_me");
    std::fs::create_dir(&watch_dir).unwrap();

    // 1. Ensure debug is disabled (default)
    // 2. Start watch in background
    let output = Command::new(ecotokens())
        .arg("watch")
        .arg("--path")
        .arg(&watch_dir)
        .arg("--background")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens watch");

    assert!(output.status.success());

    // Wait a bit for the daemon to start and potentially create the log
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 3. Check ~/.config/ecotokens/ for log files
    let config_dir = config_home.join("ecotokens");
    if config_dir.exists() {
        let entries = std::fs::read_dir(config_dir).unwrap();
        for entry in entries {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) == Some("log") {
                panic!(
                    "Log file should not exist when debug is disabled: {:?}",
                    path
                );
            }
        }
    }

    // 4. Stop watch
    let _ = Command::new(ecotokens())
        .arg("watch")
        .arg("--stop")
        .env("XDG_CONFIG_HOME", config_home)
        .output();
}

#[test]
fn test_watch_log_created_when_debug_enabled() {
    let tmp = tempdir().expect("failed to create temp dir");
    let config_home = tmp.path();
    let watch_dir = tmp.path().join("watch_me_debug");
    std::fs::create_dir(&watch_dir).unwrap();

    // 1. Enable debug
    let _ = Command::new(ecotokens())
        .arg("config")
        .arg("--debug")
        .arg("true")
        .env("XDG_CONFIG_HOME", config_home)
        .output();

    // 2. Start watch in background
    let output = Command::new(ecotokens())
        .arg("watch")
        .arg("--path")
        .arg(&watch_dir)
        .arg("--background")
        .env("XDG_CONFIG_HOME", config_home)
        .output()
        .expect("failed to run ecotokens watch");

    assert!(output.status.success());

    // Wait for daemon to start, then trigger an event.
    // Use a longer initial wait so the watcher is registered before the write.
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // Trigger an event
    std::fs::write(watch_dir.join("test.rs"), "fn main() {}").unwrap();

    // Poll for the log file for up to 10 seconds (covers slow CI runners).
    let config_dir = config_home.join("ecotokens");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut log_found = false;
    while std::time::Instant::now() < deadline {
        if config_dir.exists() {
            let entries = std::fs::read_dir(&config_dir).unwrap();
            for entry in entries {
                let path = entry.unwrap().path();
                if path.extension().and_then(|s| s.to_str()) == Some("log") {
                    log_found = true;
                    break;
                }
            }
        }
        if log_found {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // Stop first to cleanup
    let _ = Command::new(ecotokens())
        .arg("watch")
        .arg("--stop")
        .env("XDG_CONFIG_HOME", config_home)
        .output();

    assert!(
        log_found,
        "Log file should be created when debug is enabled"
    );
}
