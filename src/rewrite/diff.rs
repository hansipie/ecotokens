// Unified-diff generation, masked persistence, and retention pruning for the
// opt-in transformation audit trail (User Story 3, FR-021 to FR-028).

use std::io;
use std::path::{Path, PathBuf};

/// Header fields recorded above the unified diff body (data-model.md
/// `TransformationDiff.header`).
pub struct DiffMetadata<'a> {
    pub mode: &'a str,
    pub target: Option<&'a str>,
    pub model: &'a str,
    pub chunk_count: usize,
    pub outcome: &'a str,
}

/// Build the full diff file content: a metadata header followed by a unified
/// diff with 3 lines of context. Masking is applied to **both sides before
/// diffing**, never after — a secret must never reach the file in any form,
/// including as a removed line (FR-024, research.md §8).
pub fn build_diff(original: &str, transformed: &str, meta: &DiffMetadata) -> String {
    let (masked_original, _) = crate::masking::mask(original);
    let (masked_transformed, _) = crate::masking::mask(transformed);

    let timestamp = chrono::Utc::now().to_rfc3339();
    let header = format!(
        "mode: {}\ntarget: {}\nmodel: {}\ntimestamp: {}\nchunk_count: {}\noutcome: {}\n\n",
        meta.mode,
        meta.target.unwrap_or(""),
        meta.model,
        timestamp,
        meta.chunk_count,
        meta.outcome,
    );

    let text_diff = similar::TextDiff::from_lines(&masked_original, &masked_transformed);
    let body = text_diff
        .unified_diff()
        .context_radius(3)
        .header("original", "transformed")
        .to_string();

    format!("{header}{body}")
}

/// Persist diff content as `{diff_dir}/ecotokens-rewrite-{RFC3339-ms}-{uuid4}.diff`
/// with mode `0600` on Unix (FR-025, FR-026). On non-Unix platforms the file
/// is written with default permissions — Windows has no equivalent of Unix
/// file modes, so callers are expected to point `diff_dir` at a user-scoped
/// location there (contracts/config-settings.md, research.md §8).
pub fn save_diff(content: &str, diff_dir: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(diff_dir)?;

    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let filename = format!(
        "ecotokens-rewrite-{timestamp}-{}.diff",
        uuid::Uuid::new_v4()
    );
    let path = diff_dir.join(filename);

    std::fs::write(&path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    Ok(path)
}

const DIFF_PREFIX: &str = "ecotokens-rewrite-";
const DIFF_SUFFIX: &str = ".diff";

/// Prune saved diffs oldest-first by mtime down to `retention` entries.
/// `retention == 0` disables pruning entirely (unbounded) (FR-027).
pub fn prune_retention(diff_dir: &Path, retention: u32) -> io::Result<()> {
    if retention == 0 {
        return Ok(());
    }

    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = match std::fs::read_dir(diff_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name = name.to_string_lossy();
                name.starts_with(DIFF_PREFIX) && name.ends_with(DIFF_SUFFIX)
            })
            .filter_map(|e| {
                let mtime = e.metadata().ok()?.modified().ok()?;
                Some((e.path(), mtime))
            })
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };

    let retention = retention as usize;
    if entries.len() <= retention {
        return Ok(());
    }

    entries.sort_by_key(|(_, mtime)| *mtime);
    let excess = entries.len() - retention;
    for (path, _) in entries.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// One resolved request to build, save, and prune a transformation diff —
/// bundles everything [`write_diff`] needs so the call site in
/// `rewrite::rewrite` stays a single call.
pub struct DiffRequest<'a> {
    pub original: &'a str,
    pub transformed: &'a str,
    pub mode: &'a str,
    pub target: Option<&'a str>,
    pub model: &'a str,
    pub chunk_count: usize,
    pub diff_dir: &'a Path,
    pub retention: u32,
}

/// Build, persist, and prune in one call. Retention pruning failures are
/// swallowed (best-effort housekeeping) — only the save itself can fail the
/// call, and even that must never block the caller's primary output
/// (FR-028); the caller is expected to degrade a write failure to a stderr
/// warning rather than propagate it further.
pub fn write_diff(req: DiffRequest) -> io::Result<PathBuf> {
    let content = build_diff(
        req.original,
        req.transformed,
        &DiffMetadata {
            mode: req.mode,
            target: req.target,
            model: req.model,
            chunk_count: req.chunk_count,
            outcome: "transformed",
        },
    );
    let path = save_diff(&content, req.diff_dir)?;
    let _ = prune_retention(req.diff_dir, req.retention);
    Ok(path)
}
