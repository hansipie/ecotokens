pub mod ai_summary;
pub mod aws;
pub mod cargo;
pub mod config_file;
pub mod container;
pub mod cpp;
pub mod db;
pub mod fs;
pub mod generic;
pub mod gh;
pub mod git;
pub mod go;
pub mod grep;
pub mod js;
pub mod markdown;
pub mod network;
pub mod python;

use crate::metrics::store::CommandFamily;

fn is_cpp_command(command: &str) -> bool {
    use std::path::Path;
    let Some(program) = command.split_whitespace().next() else {
        return false;
    };
    let Some(program) = Path::new(program).file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        program,
        "gcc"
            | "g++"
            | "cc"
            | "c++"
            | "clang"
            | "clang++"
            | "clang-cl"
            | "make"
            | "cmake"
            | "ninja"
    )
}

pub fn detect_family(command: &str) -> CommandFamily {
    let cmd = command.trim();
    if cmd.starts_with("git ") {
        CommandFamily::Git
    } else if cmd.starts_with("cargo ") {
        CommandFamily::Cargo
    } else if is_cpp_command(cmd) {
        CommandFamily::Cpp
    } else if cmd.starts_with("python")
        || cmd.starts_with("pytest")
        || cmd.starts_with("pip ")
        || cmd.starts_with("ruff ")
        || cmd.starts_with("mypy")
        || cmd.starts_with("uv ")
    {
        CommandFamily::Python
    } else if cmd.starts_with("ls")
        || cmd.starts_with("find")
        || cmd.starts_with("tree")
        || cmd.starts_with("diff ")
        || cmd.starts_with("wc")
    {
        CommandFamily::Fs
    } else if cmd.starts_with("go ") || cmd.contains("golangci-lint") {
        CommandFamily::Go
    } else if cmd.starts_with("npm ")
        || cmd.starts_with("pnpm ")
        || cmd.starts_with("npx ")
        || cmd.starts_with("tsc")
        || cmd.starts_with("vitest")
        || cmd.starts_with("eslint")
        || cmd.starts_with("prettier")
        || cmd.starts_with("next ")
        || cmd.contains("playwright")
        || cmd.contains("prisma")
    {
        CommandFamily::Js
    } else if cmd.starts_with("gh ") {
        CommandFamily::Gh
    } else if cmd.starts_with("docker ")
        || cmd.starts_with("podman ")
        || cmd.starts_with("kubectl ")
    {
        CommandFamily::Container
    } else if cmd.starts_with("grep ") || cmd.starts_with("rg ") {
        CommandFamily::Grep
    } else if cmd.starts_with("aws ") {
        CommandFamily::Aws
    } else if cmd.starts_with("curl ") || cmd.starts_with("wget ") {
        CommandFamily::Network
    } else if cmd.starts_with("psql ") {
        CommandFamily::Db
    } else {
        CommandFamily::Generic
    }
}

pub fn apply_filter(command: &str, output: &str) -> String {
    let ext = std::path::Path::new(command)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match detect_family(command) {
        CommandFamily::Git => git::filter_git(command, output),
        CommandFamily::Cargo => cargo::filter_cargo(command, output),
        CommandFamily::Cpp => cpp::filter_cpp(command, output),
        CommandFamily::Python => python::filter_python(command, output),
        CommandFamily::Fs => fs::filter_fs(command, output),
        CommandFamily::Markdown => markdown::filter_markdown(output),
        CommandFamily::ConfigFile => config_file::filter_config_file(output, ext),
        CommandFamily::Go => go::filter_go(command, output),
        CommandFamily::Js => js::filter_js(command, output),
        CommandFamily::Gh => gh::filter_gh(command, output),
        CommandFamily::Container => container::filter_container(command, output),
        CommandFamily::Grep => grep::filter_grep(output),
        CommandFamily::Aws => aws::filter_aws(output),
        CommandFamily::Network => network::filter_network(command, output),
        CommandFamily::Db => db::filter_db(output),
        CommandFamily::Generic => generic::filter_generic(output, 200, 51200),
        CommandFamily::NativeRead => output.to_string(),
    }
}

/// Run the full filter pipeline with an optional working directory for git_root detection.
/// Returns `(filtered_output, tokens_before, tokens_after)`.
pub fn run_filter_pipeline_with_cwd(
    command: &str,
    raw: &str,
    duration_ms: u32,
    cwd: Option<&std::path::Path>,
) -> (String, u32, u32) {
    let settings = crate::config::Settings::load();
    let (masked, redacted) = crate::masking::mask(raw);
    let filtered = if raw.chars().count() < 200 {
        masked.clone()
    } else {
        let mut f = apply_filter(command, &masked);
        let masked_tokens = crate::tokens::count_tokens(&masked) as u32;
        let filtered_tokens = crate::tokens::count_tokens(&f) as u32;
        if f == masked || (masked_tokens > 0 && filtered_tokens >= masked_tokens) {
            f = ai_summary::ai_summary_or_fallback(&masked, &settings);
        }
        f
    };

    let tokens_before = crate::tokens::count_tokens(raw) as u32;
    let filtered_tokens = crate::tokens::count_tokens(&filtered) as u32;
    let (filtered, tokens_after) = if filtered_tokens > tokens_before {
        (masked.clone(), crate::tokens::count_tokens(&masked) as u32)
    } else {
        (filtered, filtered_tokens)
    };

    #[cfg(not(test))]
    if let Some(path) = crate::metrics::store::metrics_path() {
        let mode = if tokens_after < tokens_before {
            crate::metrics::store::FilterMode::Filtered
        } else {
            #[cfg(feature = "ai-summary")]
            {
                crate::metrics::store::FilterMode::Summarized
            }
            #[cfg(not(feature = "ai-summary"))]
            {
                crate::metrics::store::FilterMode::Filtered
            }
        };
        let family = detect_family(command);
        let effective_cwd = cwd
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        let git_root = effective_cwd.as_deref().map(|dir| {
            std::process::Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .current_dir(dir)
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
                .unwrap_or_else(|| dir.to_string_lossy().to_string())
        });
        let rec = crate::metrics::store::Interception::new(
            command.to_string(),
            family,
            git_root,
            tokens_before,
            tokens_after,
            mode,
            redacted,
            duration_ms,
            Some(masked),
            Some(filtered.clone()),
        );
        let _ = crate::metrics::store::append_to(&path, &rec);
    }

    (filtered, tokens_before, tokens_after)
}
