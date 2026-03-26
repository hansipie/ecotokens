use ecotokens::hook::glob_handler::handle_glob;
use ecotokens::hook::post_handler::PostFilterResult;

#[test]
fn glob_no_noisy_dirs_passthrough() {
    let filenames = "src/main.rs\nsrc/lib.rs\ntests/foo_test.rs";
    let result = handle_glob(filenames);
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "clean file list should passthrough (nothing to filter)"
    );
}

#[test]
fn glob_empty_passthrough() {
    let result = handle_glob("");
    assert!(
        matches!(result, PostFilterResult::Passthrough),
        "empty file list should passthrough"
    );
}

#[test]
fn glob_excludes_node_modules() {
    let filenames = "node_modules/lodash/index.js\nsrc/main.rs\nnode_modules/react/index.js";
    let result = handle_glob(filenames);
    // RED with stub — stub returns Passthrough, but we expect Filtered
    match result {
        PostFilterResult::Filtered { output, .. } => {
            // No actual path should reference node_modules (annotation is ok)
            let path_lines: Vec<&str> = output
                .lines()
                .filter(|l| !l.starts_with("[ecotokens"))
                .collect();
            assert!(
                path_lines.iter().all(|p| !p.contains("node_modules")),
                "filtered paths should not contain node_modules, got: {:?}",
                path_lines
            );
            assert!(
                output.contains("src/main.rs"),
                "filtered output should still contain non-noisy files"
            );
        }
        PostFilterResult::Passthrough => {
            panic!("expected Filtered (node_modules should be excluded), got Passthrough — expected RED with stub");
        }
    }
}

#[test]
fn glob_excludes_target_dir() {
    let filenames = "target/debug/ecotokens\nsrc/main.rs\ntarget/release/ecotokens";
    let result = handle_glob(filenames);
    match result {
        PostFilterResult::Filtered { output, .. } => {
            let path_lines: Vec<&str> = output
                .lines()
                .filter(|l| !l.starts_with("[ecotokens"))
                .collect();
            assert!(
                path_lines.iter().all(|p| !p.starts_with("target/")),
                "target/ paths should be excluded"
            );
            assert!(output.contains("src/main.rs"), "src/ should remain");
        }
        PostFilterResult::Passthrough => {
            panic!("target/ entries should be filtered — RED with stub");
        }
    }
}

#[test]
fn glob_all_noisy_returns_filtered_empty() {
    let filenames = "node_modules/foo/bar.js\ntarget/debug/ecotokens\n.git/HEAD";
    let result = handle_glob(filenames);
    match result {
        PostFilterResult::Filtered {
            output,
            tokens_before,
            ..
        } => {
            assert!(
                tokens_before > 0,
                "tokens_before should count original entries"
            );
            // Output may be empty or contain exclusion annotation
            let _ = output;
        }
        PostFilterResult::Passthrough => {
            panic!("all-noisy list should be Filtered — RED with stub");
        }
    }
}
