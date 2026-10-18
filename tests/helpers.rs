use std::sync::OnceLock;

/// Path of the `ecotokens` binary under test.
///
/// Also points `XDG_CONFIG_HOME` at a throwaway directory for the whole test
/// process, so a spawned binary (and any in-process code reading the config dir)
/// never touches the real `~/.config/ecotokens`: its config, metrics, debug log,
/// index and sessions. A test that needs its own directory still sets
/// `XDG_CONFIG_HOME` on its `Command`, which overrides this one.
pub fn ecotokens_bin() -> String {
    isolate_config_dir();
    env!("CARGO_BIN_EXE_ecotokens").to_string()
}

fn isolate_config_dir() {
    // A static is never dropped: the directory outlives the process and is left
    // to the OS temp cleaner, which is what every test that spawns the binary needs.
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("create a throwaway config dir");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        dir
    });
}
