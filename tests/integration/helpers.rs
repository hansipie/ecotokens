#[path = "../helpers.rs"]
mod base;

/// Path of the `ecotokens` binary under test, with the config dir isolated
/// (see `tests/helpers.rs`).
pub fn ecotokens() -> String {
    base::ecotokens_bin()
}
