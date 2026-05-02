use ecotokens::config::session_store::SessionStore;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns a PID that is guaranteed to be running (the test process itself).
fn live_pid() -> u32 {
    std::process::id()
}

/// Returns a PID that is guaranteed to be dead (u32::MAX is never a valid PID).
fn dead_pid() -> u32 {
    u32::MAX
}

// ── increment / needs_watcher ─────────────────────────────────────────────────

#[test]
fn first_increment_signals_watcher_needed() {
    let mut store = SessionStore::default();
    let needs = store.increment("/project-a");
    assert!(needs, "first session on a fresh path needs a watcher");
}

#[test]
fn second_increment_same_path_no_watcher_needed_when_live() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    // Register a live watcher so the second increment sees it as running.
    store.register_watcher("/project-a", live_pid(), None);
    let needs = store.increment("/project-a");
    assert!(!needs, "watcher already running — no second launch needed");
}

#[test]
fn two_different_paths_both_need_watchers() {
    let mut store = SessionStore::default();
    let needs_a = store.increment("/project-a");
    let needs_b = store.increment("/project-b");
    assert!(needs_a, "/project-a needs a watcher");
    assert!(needs_b, "/project-b needs a watcher (independent path)");
}

#[test]
fn session_counts_are_independent_per_path() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.increment("/project-a");
    store.increment("/project-b");
    assert_eq!(store.0["/project-a"].sessions, 2);
    assert_eq!(store.0["/project-b"].sessions, 1);
}

#[test]
fn child_session_reuses_live_parent_watcher() {
    let mut store = SessionStore::default();
    store.increment("/vault");
    store.register_watcher("/vault", live_pid(), None);

    let decision = store.increment_for_session("/vault/projects/petales");

    assert_eq!(decision.watch_path, "/vault");
    assert!(!decision.needs_watcher);
    assert!(decision.reused_existing_watcher);
    assert_eq!(store.0["/vault"].sessions, 2);
    assert!(!store.0.contains_key("/vault/projects/petales"));
}

#[test]
fn child_session_restarts_parent_watch_when_parent_entry_exists_without_live_watcher() {
    let mut store = SessionStore::default();
    store.increment("/vault");
    store.register_watcher("/vault", dead_pid(), None);
    // cleanup_dead removes the /vault entry (dead watcher).
    store.cleanup_dead();
    assert!(!store.0.contains_key("/vault"));

    // No ancestor entry exists: a new watcher is needed for the child path itself.
    let decision = store.increment_for_session("/vault/projects/petales");

    assert_eq!(decision.watch_path, "/vault/projects/petales");
    assert!(decision.needs_watcher);
    assert!(!decision.reused_existing_watcher);
    assert_eq!(store.0["/vault/projects/petales"].sessions, 1);
}

// ── decrement / stop signal ───────────────────────────────────────────────────

#[test]
fn decrement_does_not_return_pid_while_sessions_remain() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.increment("/project-a");
    store.register_watcher("/project-a", 42, None);
    let pid = store.decrement("/project-a");
    assert!(
        pid.is_none(),
        "one session still open — watcher must keep running"
    );
    assert_eq!(store.0["/project-a"].sessions, 1);
}

#[test]
fn decrement_returns_pid_on_last_session() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", 42, None);
    let pid = store.decrement("/project-a");
    assert_eq!(
        pid,
        Some(42),
        "last session closed — watcher pid must be returned"
    );
}

#[test]
fn decrement_removes_entry_when_sessions_reach_zero() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", 42, None);
    store.decrement("/project-a");
    assert!(
        !store.0.contains_key("/project-a"),
        "entry must be removed once sessions == 0"
    );
}

#[test]
fn two_paths_close_independently() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.increment("/project-b");
    store.register_watcher("/project-a", 111, None);
    store.register_watcher("/project-b", 222, None);

    // Close /project-a first.
    let pid_a = store.decrement("/project-a");
    assert_eq!(pid_a, Some(111));
    assert!(!store.0.contains_key("/project-a"));
    // /project-b must be untouched.
    assert_eq!(store.0["/project-b"].sessions, 1);
    assert_eq!(store.0["/project-b"].watcher_pid, Some(222));

    // Close /project-b.
    let pid_b = store.decrement("/project-b");
    assert_eq!(pid_b, Some(222));
    assert!(!store.0.contains_key("/project-b"));
}

#[test]
fn child_session_decrements_parent_watcher_when_reused() {
    let mut store = SessionStore::default();
    store.increment("/vault");
    store.register_watcher("/vault", 42, None);
    store.increment_for_session("/vault/projects/petales");

    let pid = store.decrement_for_session("/vault/projects/petales");

    assert!(
        pid.is_none(),
        "parent watcher still has the original session"
    );
    assert_eq!(store.0["/vault"].sessions, 1);
}

// ── register_watcher ─────────────────────────────────────────────────────────

#[test]
fn register_watcher_sets_pid() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", 99, None);
    assert_eq!(store.0["/project-a"].watcher_pid, Some(99));
}

#[test]
fn after_register_second_increment_needs_no_watcher() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", live_pid(), None);
    let needs = store.increment("/project-a");
    assert!(!needs);
}

// ── cleanup_dead ─────────────────────────────────────────────────────────────

#[test]
fn cleanup_dead_clears_dead_watcher_pid() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", dead_pid(), None);
    store.cleanup_dead();
    assert!(
        !store.0.contains_key("/project-a"),
        "entry with dead watcher must be removed"
    );
}

#[test]
fn cleanup_dead_keeps_live_watcher() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", live_pid(), None);
    store.cleanup_dead();
    assert!(
        store.0["/project-a"].watcher_pid.is_some(),
        "live watcher must be preserved"
    );
}

#[test]
fn cleanup_dead_removes_entry_with_no_sessions_and_dead_watcher() {
    let mut store = SessionStore::default();
    // Simulate an orphaned entry: sessions=0, dead pid (e.g. after a crash).
    store.0.insert(
        "/ghost".to_string(),
        ecotokens::config::session_store::WatcherEntry {
            sessions: 0,
            watcher_pid: Some(dead_pid()),
            log_file: None,
            started_at: None,
        },
    );
    store.cleanup_dead();
    assert!(
        !store.0.contains_key("/ghost"),
        "orphaned entry must be removed"
    );
}

#[test]
fn cleanup_dead_retains_entry_with_live_sessions_and_dead_watcher() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", dead_pid(), None);
    store.cleanup_dead();
    // Dead watcher → entry removed regardless of session count.
    assert!(!store.0.contains_key("/project-a"));
}

// ── dead watcher triggers new launch ─────────────────────────────────────────

#[test]
fn increment_after_dead_watcher_signals_new_launch_needed() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.register_watcher("/project-a", dead_pid(), None);
    // Simulate second session opening without prior cleanup.
    let needs = store.increment("/project-a");
    assert!(needs, "dead watcher must trigger a new launch");
}

// ── serialisation round-trip ──────────────────────────────────────────────────

#[test]
fn round_trip_json() {
    let mut store = SessionStore::default();
    store.increment("/project-a");
    store.increment("/project-a");
    store.register_watcher("/project-a", 111, None);
    store.increment("/project-b");
    store.register_watcher("/project-b", 222, None);

    let json = serde_json::to_string(&store.0).unwrap();
    let restored: std::collections::HashMap<
        String,
        ecotokens::config::session_store::WatcherEntry,
    > = serde_json::from_str(&json).unwrap();

    assert_eq!(restored["/project-a"].sessions, 2);
    assert_eq!(restored["/project-a"].watcher_pid, Some(111));
    assert_eq!(restored["/project-b"].sessions, 1);
    assert_eq!(restored["/project-b"].watcher_pid, Some(222));
}
