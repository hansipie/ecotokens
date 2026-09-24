#[path = "../helpers.rs"]
mod helpers;
use helpers::ecotokens_bin;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use tempfile::tempdir;

/// A minimal fake Ollama server: reads (and discards) the request, then
/// replies with a valid `{"response": "..."}` body so the CLI's real
/// `OllamaProvider` completes a genuine `Status::Transformed` result — no
/// stub injection point exists at the subprocess boundary (see
/// tests/integration/rewrite_failopen_test.rs for the same pattern used to
/// simulate failures; this simulates success instead).
fn spawn_fake_ollama(response_text: &str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let body = serde_json::json!({ "response": response_text }).to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let http = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(http.as_bytes());
        }
    });
    port
}

struct RunOutcome {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_rewrite(
    input: &str,
    url: &str,
    diff_dir: Option<&std::path::Path>,
    save_diff: bool,
    xdg_config_home: &std::path::Path,
) -> RunOutcome {
    let config_dir = xdg_config_home.join("ecotokens");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    let diff_dir_json = diff_dir
        .map(|d| format!(r#", "rewrite_diff_dir": {:?}"#, d.to_string_lossy()))
        .unwrap_or_default();
    std::fs::write(
        config_dir.join("config.json"),
        format!(r#"{{"rewrite_url": "{url}"{diff_dir_json}}}"#),
    )
    .expect("write config.json");

    let mut args = vec!["rewrite", "--mode", "paraphrase", "--json"];
    if save_diff {
        args.push("--save-diff");
    }

    let mut child = Command::new(ecotokens_bin())
        .args(&args)
        .env("XDG_CONFIG_HOME", xdg_config_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ecotokens rewrite");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    RunOutcome {
        status_code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn diff_files_in(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ecotokens-rewrite-") && n.ends_with(".diff"))
        })
        .collect()
}

#[test]
fn fresh_install_with_default_config_writes_zero_diff_files() {
    // Nothing listening — this exercises fail-open (Fallback), which must
    // never attempt a diff write. Default rewrite_save_diff is false and
    // --save-diff is not passed, so even a successful transform must not
    // write anything (covered separately below).
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let out = run_rewrite(
        "Some prose input.",
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        false,
        xdg.path(),
    );
    assert_eq!(out.status_code, Some(0));
    assert!(diff_files_in(diff_dir.path()).is_empty());
}

#[test]
fn successful_transform_without_save_diff_flag_writes_nothing() {
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let port = spawn_fake_ollama("Transformed output text.");

    let out = run_rewrite(
        "Original input text.",
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        false, // no --save-diff, and config default (rewrite_save_diff) is false
        xdg.path(),
    );
    assert_eq!(out.status_code, Some(0), "stderr: {}", out.stderr);
    assert!(diff_files_in(diff_dir.path()).is_empty());
}

#[test]
fn save_diff_flag_writes_a_diff_containing_the_change() {
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let port = spawn_fake_ollama("Transformed output text.");

    let out = run_rewrite(
        "Original input text.",
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );
    assert_eq!(out.status_code, Some(0), "stderr: {}", out.stderr);
    let files = diff_files_in(diff_dir.path());
    assert_eq!(
        files.len(),
        1,
        "expected exactly one diff file, stderr: {}",
        out.stderr
    );
    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(content.contains("Original input text"));
    assert!(content.contains("Transformed output text"));

    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        v["diff_path"]
            .as_str()
            .map(std::path::PathBuf::from)
            .as_ref(),
        Some(&files[0])
    );
}

#[test]
fn secrets_are_masked_on_both_sides_of_the_saved_diff() {
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let secret = "AKIAABCD1234EFGH5678";
    let port = spawn_fake_ollama(&format!("Kindly use key {secret} for access."));

    let out = run_rewrite(
        &format!("Please use key {secret} for access."),
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );
    assert_eq!(out.status_code, Some(0), "stderr: {}", out.stderr);
    let files = diff_files_in(diff_dir.path());
    assert_eq!(files.len(), 1);
    let content = std::fs::read_to_string(&files[0]).unwrap();
    assert!(
        !content.contains(secret),
        "raw secret leaked into diff file: {content}"
    );
    assert!(content.contains("[AWS_KEY]"), "got: {content}");
}

#[test]
fn concurrent_invocations_produce_distinct_files() {
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let port = spawn_fake_ollama("Transformed output text.");
    let url = format!("http://127.0.0.1:{port}");

    run_rewrite(
        "First input.",
        &url,
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );
    run_rewrite(
        "Second input.",
        &url,
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );

    let files = diff_files_in(diff_dir.path());
    assert_eq!(files.len(), 2, "got: {files:?}");
    assert_ne!(files[0], files[1]);
}

#[cfg(unix)]
#[test]
fn unwritable_directory_degrades_to_a_stderr_warning() {
    use std::os::unix::fs::PermissionsExt;
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let mut perms = std::fs::metadata(diff_dir.path()).unwrap().permissions();
    perms.set_mode(0o500); // read+execute only, not writable
    std::fs::set_permissions(diff_dir.path(), perms).unwrap();

    let port = spawn_fake_ollama("Transformed output text.");
    let out = run_rewrite(
        "Original input text.",
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );

    // Restore permissions so the tempdir can be cleaned up.
    let mut perms = std::fs::metadata(diff_dir.path()).unwrap().permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(diff_dir.path(), perms).unwrap();

    assert_eq!(
        out.status_code,
        Some(0),
        "write failure must never change exit code"
    );
    let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(
        v["status"], "transformed",
        "stdout must still carry the transformed text"
    );
    assert!(
        !out.stderr.is_empty(),
        "expected a diff-write warning on stderr"
    );
}

#[cfg(unix)]
#[test]
fn saved_diff_has_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let xdg = tempdir().unwrap();
    let diff_dir = tempdir().unwrap();
    let port = spawn_fake_ollama("Transformed output text.");

    let out = run_rewrite(
        "Original input text.",
        &format!("http://127.0.0.1:{port}"),
        Some(diff_dir.path()),
        true,
        xdg.path(),
    );
    assert_eq!(out.status_code, Some(0), "stderr: {}", out.stderr);
    let files = diff_files_in(diff_dir.path());
    assert_eq!(files.len(), 1);
    let mode = std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "got mode {mode:o}");
}
