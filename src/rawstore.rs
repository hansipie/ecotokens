//! Recovery of the full output behind a filtered result (`ecotokens show <id>`).
//!
//! Only the secret-masked text is ever written to disk, so recovering an
//! output never reintroduces a secret that filtering would have hidden. Files
//! live in `<config_dir>/ecotokens/raw/<id>.txt` with mode 0600 and are pruned
//! by age and by count each time a new one is saved.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Length of the short id shown to users.
const ID_LEN: usize = 8;
/// A single saved output is capped to keep the store bounded.
const MAX_ENTRY_BYTES: usize = 2 * 1024 * 1024;
/// Filtering must save at least this share of tokens before a hint is shown.
const MIN_SAVINGS_PCT: u32 = 20;

pub fn raw_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ecotokens").join("raw"))
}

/// True when the filtered result is small enough, relative to the original,
/// that a user may reasonably want the full output back.
pub fn worth_saving(tokens_before: u32, tokens_after: u32) -> bool {
    tokens_before > 0
        && (tokens_after as u64) * 100 <= (tokens_before as u64) * (100 - MIN_SAVINGS_PCT as u64)
}

/// A valid id is exactly `ID_LEN` lowercase hex digits. Checked before any
/// path is built so `show ../x` can never escape the store.
pub fn is_valid_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..ID_LEN].to_string()
}

fn truncate_to_bytes(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…[truncated at {} bytes]", &s[..end], max)
}

/// Save `masked` (already secret-masked) in `dir` and return its id.
pub fn save_in(dir: &Path, masked: &str) -> io::Result<String> {
    std::fs::create_dir_all(dir)?;
    let id = new_id();
    let path = dir.join(format!("{id}.txt"));
    let body = truncate_to_bytes(masked, MAX_ENTRY_BYTES);

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(body.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, body)?;

    Ok(id)
}

/// Read a saved output from `dir`.
pub fn load_from(dir: &Path, id: &str) -> io::Result<String> {
    if !is_valid_id(id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid id '{id}' (expected {ID_LEN} hex characters)"),
        ));
    }
    std::fs::read_to_string(dir.join(format!("{id}.txt")))
}

/// Delete entries older than `retention_days` and, beyond `max_entries`, the
/// oldest ones. A limit of `0` disables that rule. Returns how many files were
/// removed. Best effort: unreadable entries are skipped.
pub fn prune_in(dir: &Path, retention_days: u32, max_entries: u32) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut entries: Vec<(SystemTime, PathBuf)> = read
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    entries.sort_by_key(|(t, _)| std::cmp::Reverse(*t)); // newest first

    let cutoff = (retention_days > 0)
        .then(|| SystemTime::now().checked_sub(Duration::from_secs(retention_days as u64 * 86_400)))
        .flatten();

    let mut removed = 0;
    for (i, (mtime, path)) in entries.iter().enumerate() {
        let too_many = max_entries > 0 && i >= max_entries as usize;
        let too_old = cutoff.is_some_and(|c| *mtime < c);
        if (too_many || too_old) && std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Delete every saved output. Returns how many files were removed.
pub fn clear_in(dir: &Path) -> usize {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    read.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "txt"))
        .filter(|e| std::fs::remove_file(e.path()).is_ok())
        .count()
}

/// The line appended to filtered output.
#[cfg_attr(test, allow(dead_code))]
pub fn hint(id: &str) -> String {
    format!("[ecotokens] Full output saved: ecotokens show {id}")
}

/// Save to the default store, prune it, and return the hint line. `None` when
/// the store is unavailable: recovery is a convenience, never a failure.
#[cfg_attr(test, allow(dead_code))]
pub fn save_and_hint(masked: &str, retention_days: u32, max_entries: u32) -> Option<String> {
    let dir = raw_dir()?;
    let id = save_in(&dir, masked).ok()?;
    prune_in(&dir, retention_days, max_entries);
    Some(hint(&id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_roundtrips() {
        let d = tempfile::tempdir().unwrap();
        let id = save_in(d.path(), "hello\nworld").unwrap();
        assert!(is_valid_id(&id));
        assert_eq!(load_from(d.path(), &id).unwrap(), "hello\nworld");
    }

    #[test]
    fn load_rejects_path_traversal_and_bad_ids() {
        let d = tempfile::tempdir().unwrap();
        for bad in ["../etc/passwd", "ABCDEF12", "abc", "abcdef123", ""] {
            let e = load_from(d.path(), bad).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
    }

    #[test]
    fn load_unknown_id_is_not_found() {
        let d = tempfile::tempdir().unwrap();
        let e = load_from(d.path(), "deadbeef").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn saved_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let id = save_in(d.path(), "x").unwrap();
        let mode = std::fs::metadata(d.path().join(format!("{id}.txt")))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn oversized_entry_is_truncated_on_a_char_boundary() {
        let d = tempfile::tempdir().unwrap();
        let big = "é".repeat(MAX_ENTRY_BYTES); // 2 bytes per char
        let id = save_in(d.path(), &big).unwrap();
        let got = load_from(d.path(), &id).unwrap();
        assert!(got.contains("[truncated at"));
        assert!(got.len() <= MAX_ENTRY_BYTES + 64);
    }

    #[test]
    fn prune_keeps_only_newest_max_entries() {
        let d = tempfile::tempdir().unwrap();
        let mut ids = vec![];
        for i in 0..5 {
            ids.push(save_in(d.path(), &format!("e{i}")).unwrap());
            std::thread::sleep(Duration::from_millis(15));
        }
        assert_eq!(prune_in(d.path(), 0, 2), 3);
        assert!(load_from(d.path(), &ids[4]).is_ok());
        assert!(load_from(d.path(), &ids[3]).is_ok());
        assert!(load_from(d.path(), &ids[0]).is_err());
    }

    #[test]
    fn prune_removes_entries_older_than_retention() {
        let d = tempfile::tempdir().unwrap();
        let old = save_in(d.path(), "old").unwrap();
        let fresh = save_in(d.path(), "fresh").unwrap();
        let f = std::fs::File::options()
            .write(true)
            .open(d.path().join(format!("{old}.txt")))
            .unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(10 * 86_400))
            .unwrap();
        assert_eq!(prune_in(d.path(), 7, 0), 1);
        assert!(load_from(d.path(), &old).is_err());
        assert!(load_from(d.path(), &fresh).is_ok());
    }

    #[test]
    fn prune_zero_limits_disable_pruning() {
        let d = tempfile::tempdir().unwrap();
        for _ in 0..3 {
            save_in(d.path(), "x").unwrap();
        }
        assert_eq!(prune_in(d.path(), 0, 0), 0);
    }

    #[test]
    fn clear_removes_everything() {
        let d = tempfile::tempdir().unwrap();
        for _ in 0..3 {
            save_in(d.path(), "x").unwrap();
        }
        assert_eq!(clear_in(d.path()), 3);
    }

    #[test]
    fn hint_only_when_savings_are_meaningful() {
        assert!(worth_saving(1000, 100));
        assert!(worth_saving(1000, 800));
        assert!(!worth_saving(1000, 801));
        assert!(!worth_saving(0, 0));
    }
}
