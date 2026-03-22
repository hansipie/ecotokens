use crate::search::index::{build_schema, open_or_create_index};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tantivy::doc;

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
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        for path in e.paths {
                            if path.is_file() {
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
            let status = reindex_single_file(&path, watch_path, index_dir);
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

/// Ré-indexe un seul fichier dans l'index tantivy.
fn reindex_single_file(path: &Path, watch_path: &Path, index_dir: &Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !is_indexable_extension(ext) {
        return "ignored".to_string();
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return format!("error: {e}"),
    };

    let rel_path = path
        .strip_prefix(watch_path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned());

    let index = match open_or_create_index(index_dir, false) {
        Ok(i) => i,
        Err(e) => return format!("error: {e}"),
    };

    let (_, file_path_field, content_field, kind_field, line_start_field, _) = build_schema();

    let mut writer = match index.writer(50_000_000) {
        Ok(w) => w,
        Err(e) => return format!("error: {e}"),
    };

    // Supprimer les anciens chunks de ce fichier
    let term = tantivy::Term::from_field_text(file_path_field, &rel_path);
    writer.delete_term(term);

    // Ré-indexer par chunks de 50 lignes
    let lines: Vec<&str> = content.lines().collect();
    for (chunk_idx, chunk) in lines.chunks(50).enumerate() {
        let chunk_text = chunk.join("\n");
        let line_start = chunk_idx as u64 * 50;
        let _ = writer.add_document(doc!(
            file_path_field => rel_path.clone(),
            content_field   => chunk_text,
            kind_field      => "bm25",
            line_start_field => line_start,
        ));
    }

    match writer.commit() {
        Ok(_) => "re-indexed".to_string(),
        Err(e) => format!("error: {e}"),
    }
}

fn is_indexable_extension(ext: &str) -> bool {
    matches!(
        ext,
        "rs" | "py"
            | "js"
            | "ts"
            | "jsx"
            | "tsx"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "cxx"
            | "hpp"
            | "hh"
            | "hxx"
            | "md"
            | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "txt"
    )
}
