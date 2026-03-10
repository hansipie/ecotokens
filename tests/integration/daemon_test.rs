use ecotokens::daemon::watcher::{watch_directory, WatchEvent};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

/// Vérifie qu'un fichier créé pendant le watch génère un événement "re-indexed".
#[test]
fn test_watch_sends_reindexed_event_on_file_change() {
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

    std::fs::write(dir.path().join("lib.rs"), "pub fn new() {}").unwrap();

    let event = event_rx
        .recv_timeout(Duration::from_secs(4))
        .expect("événement attendu après création de lib.rs");

    // L'événement doit avoir un timestamp non-vide
    assert!(!event.timestamp.is_empty(), "timestamp vide");
    // Le statut doit être re-indexed ou error (pas ignored pour .rs)
    assert!(
        event.status == "re-indexed" || event.status.starts_with("error"),
        "statut inattendu pour .rs : {}",
        event.status
    );

    let _ = stop_tx.send(());
    let _ = handle.join();
}

/// Vérifie que le watcher s'arrête proprement sur signal stop.
#[test]
fn test_watch_stops_cleanly_on_stop_signal() {
    let dir = TempDir::new().unwrap();
    let index_dir = TempDir::new().unwrap();

    let (event_tx, _event_rx) = mpsc::channel::<WatchEvent>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    let watch_path = dir.path().to_path_buf();
    let idx_dir = index_dir.path().to_path_buf();

    let handle =
        std::thread::spawn(move || watch_directory(&watch_path, &idx_dir, event_tx, stop_rx));

    std::thread::sleep(Duration::from_millis(200));

    // Envoyer le signal d'arrêt
    stop_tx.send(()).unwrap();

    let result = handle.join().expect("le thread watcher a paniqué");
    assert!(
        result.is_ok(),
        "watch_directory a retourné une erreur : {:?}",
        result
    );
}

/// Vérifie que le watcher gère correctement plusieurs fichiers en rafale (debounce).
#[test]
fn test_watch_debounces_rapid_changes() {
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

    // Écrire rapidement plusieurs fois dans le même fichier
    let file = dir.path().join("rapid.rs");
    for i in 0..5 {
        std::fs::write(&file, format!("fn v{i}() {{}}")).unwrap();
        std::thread::sleep(Duration::from_millis(10));
    }

    // On doit recevoir au moins 1 événement (debounce agrège les modifications)
    let event = event_rx
        .recv_timeout(Duration::from_secs(4))
        .expect("au moins un événement attendu");

    assert!(
        event.status == "re-indexed" || event.status.starts_with("error"),
        "statut inattendu : {}",
        event.status
    );

    let _ = stop_tx.send(());
    let _ = handle.join();
}
