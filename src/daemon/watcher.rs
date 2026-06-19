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
    /// "re-indexed", "ignored", ou "error: <message>"
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

        for path in ready {
            pending.remove(&path);
            let ts = chrono::Utc::now().format("%H:%M:%S").to_string();
            let status = reindex_incremental(&path, watch_path, index_dir, &embed_provider);

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

/// Ré-indexe le projet en mode incrémental pour garder BM25, symboles et HNSW cohérents.
fn reindex_incremental(
    path: &Path,
    watch_path: &Path,
    index_dir: &Path,
    embed_provider: &EmbedProvider,
) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !is_indexable_extension(ext) {
        return "ignored".to_string();
    }
    if is_gitignored(path, watch_path) {
        return "ignored".to_string();
    }

    let opts = IndexOptions {
        reset: false,
        path: watch_path.to_path_buf(),
        index_dir: index_dir.to_path_buf(),
        progress: None,
        embed_provider: embed_provider.clone(),
        log_tx: None,
    };

    match index_directory(opts) {
        Ok(_) => "re-indexed".to_string(),
        Err(e) => format!("error: {e}"),
    }
}
