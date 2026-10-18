use crate::config::settings::EmbedProvider;
use crate::search::index::{index_directory, IndexOptions};
use crate::search::is_indexable_extension;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Un événement émis par le watcher après ré-indexation d'un fichier.
#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub timestamp: String,
    /// "re-indexed" (suivi de "(N chunks without vector)" si l'embedding a échoué),
    /// "ignored", ou "error: <message>"
    pub status: String,
}

/// Lance la surveillance du répertoire `watch_path`.
///
/// Chaque fichier modifié ou créé est ré-indexé après un debounce de 500 ms.
/// La boucle s'arrête quand `stop_rx` reçoit un message ou quand `event_tx` est fermé.
pub fn watch_directory(
    watch_path: &Path,
    index_dir: &Path,
    embed_provider: EmbedProvider,
    event_tx: mpsc::Sender<WatchEvent>,
    stop_rx: mpsc::Receiver<()>,
) -> notify::Result<()> {
    use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

    let (notify_tx, notify_rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = RecommendedWatcher::new(notify_tx, Config::default())?;
    watcher.watch(watch_path, RecursiveMode::Recursive)?;

    // Debounce : path → instant du dernier événement reçu
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let debounce = Duration::from_millis(500);
    let poll_interval = Duration::from_millis(50);

    loop {
        // Vérifier le signal d'arrêt
        match stop_rx.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // Drainer les événements notify
        while let Ok(event) = notify_rx.try_recv() {
            if let Ok(e) = event {
                match e.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        for path in e.paths {
                            if path.is_file()
                                || matches!(e.kind, EventKind::Remove(_))
                                || is_indexable_path(&path)
                            {
                                pending.insert(path, Instant::now());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Traiter les fichiers dont le debounce est écoulé
        let now = Instant::now();
        let ready: Vec<PathBuf> = pending
            .iter()
            .filter(|(_, last)| now.duration_since(**last) >= debounce)
            .map(|(p, _)| p.clone())
            .collect();

        // Gate each ready path (extension + gitignore) first, then run at most a
        // single reindex for the whole debounce batch. `index_directory` walks
        // the whole project, so calling it once per changed file would repeat
        // that walk needlessly when an editor or git operation touches many files
        // at once.
        let mut statuses: Vec<(PathBuf, Option<String>)> = Vec::new();
        let mut needs_reindex = false;
        for path in &ready {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !is_indexable_extension(ext) || is_gitignored(path, watch_path) {
                statuses.push((path.clone(), Some("ignored".to_string())));
            } else {
                needs_reindex = true;
                statuses.push((path.clone(), None)); // filled in after the reindex
            }
        }
        for path in &ready {
            pending.remove(path);
        }

        let reindex_status = if needs_reindex {
            Some(run_full_index(watch_path, index_dir, &embed_provider))
        } else {
            None
        };

        for (path, gated) in statuses {
            let status = gated
                .or_else(|| reindex_status.clone())
                .unwrap_or_else(|| "re-indexed".to_string());
            // Include the date so log ordering is unambiguous across midnight.
            let ts = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            if event_tx
                .send(WatchEvent {
                    path,
                    timestamp: ts,
                    status,
                })
                .is_err()
            {
                // Receiver fermé : on arrête
                return Ok(());
            }
        }

        std::thread::sleep(poll_interval);
    }

    Ok(())
}

fn is_indexable_path(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    is_indexable_extension(ext)
}

fn is_gitignored(path: &Path, root: &Path) -> bool {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    let gitignore = root.join(".gitignore");
    if gitignore.exists() {
        let _ = builder.add(&gitignore);
    }
    let Ok(gi) = builder.build() else {
        return false;
    };
    // Path::strip_prefix (stdlib) supprime proprement le séparateur, ce qui
    // donne "target/foo.rs" et non "/target/foo.rs". Le strip_prefix interne
    // de la crate ignore est octet-par-octet et laisse un '/' résiduel qui
    // empêche le matching des patterns gitignore.
    let rel = path.strip_prefix(root).unwrap_or(path);
    // matched_path_or_any_parents remonte les composantes : "target/build.rs"
    // est ignoré parce que son parent "target" matche le pattern "target/".
    gi.matched_path_or_any_parents(rel, path.is_dir())
        .is_ignore()
}

/// Ré-indexe le projet en mode incrémental (reset:false) pour garder BM25,
/// symboles et HNSW cohérents. Le walk saute les fichiers inchangés via les
/// timestamps; l'appelant ne l'invoque qu'une fois par cycle de debounce.
fn run_full_index(watch_path: &Path, index_dir: &Path, embed_provider: &EmbedProvider) -> String {
    let opts = IndexOptions {
        reset: false,
        path: watch_path.to_path_buf(),
        index_dir: index_dir.to_path_buf(),
        progress: None,
        embed_provider: embed_provider.clone(),
        log_tx: None,
    };

    match index_directory(opts) {
        Ok(stats) if stats.embed_failed_count > 0 => format!(
            "re-indexed ({} chunks without vector)",
            stats.embed_failed_count
        ),
        Ok(_) => "re-indexed".to_string(),
        Err(e) => format!("error: {e}"),
    }
}

/// Size above which a watch log is rotated to `<name>.log.1` (one generation kept).
const WATCH_LOG_MAX_BYTES: u64 = 10 * 1024 * 1024;

/// The line to write to the watch log for `event`, or `None` for an event that
/// carries no information. "ignored" events are skipped: they are the bulk of a
/// log (a `cargo build` fires one per file under `target/`) and only say that a
/// file was skipped on purpose.
pub fn watch_log_line(event: &WatchEvent) -> Option<String> {
    if event.status == "ignored" {
        return None;
    }
    Some(format!(
        "[{}] {} {}\n",
        event.timestamp,
        event.path.display(),
        event.status
    ))
}

/// Appends `line` to the watch log at `path`, rotating the log first when it
/// exceeds [`WATCH_LOG_MAX_BYTES`] so a long-running watcher cannot fill the disk.
pub fn append_watch_log(path: &Path, line: &str) {
    append_watch_log_capped(path, line, WATCH_LOG_MAX_BYTES);
}

fn append_watch_log_capped(path: &Path, line: &str, max_bytes: u64) {
    use std::io::Write;
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max_bytes) {
        let mut rotated = path.as_os_str().to_owned();
        rotated.push(".1");
        let _ = std::fs::rename(path, rotated);
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(line.as_bytes()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(status: &str) -> WatchEvent {
        WatchEvent {
            path: PathBuf::from("/p/target/debug/x.rs"),
            timestamp: "2026-10-18 12:00:00".to_string(),
            status: status.to_string(),
        }
    }

    #[test]
    fn ignored_events_are_not_logged() {
        assert_eq!(watch_log_line(&event("ignored")), None);
    }

    #[test]
    fn other_events_are_logged_with_their_status() {
        assert_eq!(
            watch_log_line(&event("re-indexed (3 chunks without vector)")).as_deref(),
            Some(
                "[2026-10-18 12:00:00] /p/target/debug/x.rs re-indexed (3 chunks without vector)\n"
            )
        );
        assert!(watch_log_line(&event("error: boom")).is_some());
    }

    #[test]
    fn log_is_rotated_once_over_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("watch_x.log");
        append_watch_log_capped(&log, "first line\n", 20);
        append_watch_log_capped(&log, "second line\n", 20);
        // 23 bytes > cap 20: the next write starts a fresh file.
        append_watch_log_capped(&log, "third\n", 20);

        let rotated = dir.path().join("watch_x.log.1");
        assert_eq!(
            std::fs::read_to_string(&rotated).unwrap(),
            "first line\nsecond line\n"
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "third\n");
    }

    #[test]
    fn log_below_the_cap_is_appended_to() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("watch_y.log");
        append_watch_log(&log, "a\n");
        append_watch_log(&log, "b\n");
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "a\nb\n");
        assert!(!dir.path().join("watch_y.log.1").exists());
    }
}
