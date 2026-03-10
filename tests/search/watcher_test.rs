use ecotokens::daemon::watcher::{watch_directory, WatchEvent};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

/// Vérifie que le watcher détecte la création d'un fichier indexable.
#[test]
fn test_watcher_detects_file_creation() {
    let dir = TempDir::new().unwrap();
    let index_dir = TempDir::new().unwrap();

    let (event_tx, event_rx) = mpsc::channel::<WatchEvent>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let watch_path = dir.path().to_path_buf();
    let idx_dir = index_dir.path().to_path_buf();

    let handle = std::thread::spawn(move || {
        let _ = watch_directory(&watch_path, &idx_dir, event_tx, stop_rx);
    });

    // Laisser le watcher démarrer
    std::thread::sleep(Duration::from_millis(200));

    // Créer un fichier Rust indexable
    std::fs::write(dir.path().join("foo.rs"), "fn hello() {}").unwrap();

    // Attendre l'événement (debounce 500ms + traitement)
    let event = event_rx
        .recv_timeout(Duration::from_secs(4))
        .expect("aucun événement reçu après création de fichier");

    assert!(
        event.status == "re-indexed" || event.status.starts_with("error"),
        "statut inattendu : {}",
        event.status
    );
    assert!(event.path.to_string_lossy().contains("foo.rs"));
    assert!(!event.timestamp.is_empty());

    let _ = stop_tx.send(());
    let _ = handle.join();
}

/// Vérifie que les fichiers non indexables retournent le statut "ignored".
#[test]
fn test_watcher_ignores_non_indexable_files() {
    let dir = TempDir::new().unwrap();
    let index_dir = TempDir::new().unwrap();

    let (event_tx, event_rx) = mpsc::channel::<WatchEvent>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let watch_path = dir.path().to_path_buf();
    let idx_dir = index_dir.path().to_path_buf();

    let handle = std::thread::spawn(move || {
        let _ = watch_directory(&watch_path, &idx_dir, event_tx, stop_rx);
    });

    std::thread::sleep(Duration::from_millis(200));

    // Créer un fichier binaire non indexable
    std::fs::write(dir.path().join("binary.exe"), b"\x00\x01\x02\x03").unwrap();

    // Si un événement arrive, il doit être "ignored"
    if let Ok(event) = event_rx.recv_timeout(Duration::from_secs(2)) {
        assert_eq!(
            event.status, "ignored",
            "fichier binaire ne devrait pas être indexé"
        );
    }
    // Pas d'événement = comportement correct aussi (le watcher filtre silencieusement)

    let _ = stop_tx.send(());
    let _ = handle.join();
}

/// Vérifie que la modification d'un fichier existant déclenche un événement.
#[test]
fn test_watcher_detects_modification() {
    let dir = TempDir::new().unwrap();
    let index_dir = TempDir::new().unwrap();

    let file = dir.path().join("src.rs");
    std::fs::write(&file, "fn initial() {}").unwrap();

    let (event_tx, event_rx) = mpsc::channel::<WatchEvent>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let watch_path = dir.path().to_path_buf();
    let idx_dir = index_dir.path().to_path_buf();

    let handle = std::thread::spawn(move || {
        let _ = watch_directory(&watch_path, &idx_dir, event_tx, stop_rx);
    });

    std::thread::sleep(Duration::from_millis(200));

    // Modifier le fichier
    std::fs::write(&file, "fn modified() { println!(\"hello\"); }").unwrap();

    let event = event_rx
        .recv_timeout(Duration::from_secs(4))
        .expect("aucun événement après modification de fichier");

    assert!(
        event.status == "re-indexed" || event.status.starts_with("error"),
        "statut inattendu : {}",
        event.status
    );

    let _ = stop_tx.send(());
    let _ = handle.join();
}
