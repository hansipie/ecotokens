pub mod session_store;
pub mod settings;

pub use session_store::SessionStore;
pub use settings::Settings;

use std::path::PathBuf;

/// Returns the default index directory: ~/.config/ecotokens/index
pub fn default_index_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| {
            eprintln!("ecotokens: warning: could not determine config dir, falling back to '.'");
            PathBuf::from(".")
        })
        .join("ecotokens")
        .join("index")
}
