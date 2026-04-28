use std::collections::HashMap;

/// Default abbreviation dictionary targeting narrative text, logs and messages only.
/// Entries are chosen to be unambiguous (no collision with common code identifiers)
/// and to preserve readability for the model.
pub const DEFAULT_PAIRS: &[(&str, &str)] = &[
    ("function", "fn"),
    ("configuration", "config"),
    ("parameter", "param"),
    ("parameters", "params"),
    ("arguments", "args"),
    ("argument", "arg"),
    ("error", "err"),
    ("errors", "errs"),
    ("warning", "warn"),
    ("warnings", "warns"),
    ("information", "info"),
    ("environment", "env"),
    ("development", "dev"),
    ("production", "prod"),
    ("database", "db"),
    ("application", "app"),
    ("directory", "dir"),
    ("directories", "dirs"),
    ("message", "msg"),
    ("messages", "msgs"),
    ("package", "pkg"),
    ("packages", "pkgs"),
    ("dependency", "dep"),
    ("dependencies", "deps"),
    ("request", "req"),
    ("response", "resp"),
    ("variable", "var"),
    ("variables", "vars"),
    ("attribute", "attr"),
    ("attributes", "attrs"),
    ("reference", "ref"),
    ("references", "refs"),
    ("documentation", "docs"),
    ("repository", "repo"),
    ("repositories", "repos"),
    ("administrator", "admin"),
    ("administrators", "admins"),
    ("command", "cmd"),
    ("commands", "cmds"),
    ("implementation", "impl"),
    ("implementations", "impls"),
];

/// Merge the default dictionary with user-provided overrides.
/// User entries win over defaults when both define the same lowercase key.
/// Custom keys are normalized to lowercase, so mixed-case variants map to the
/// same entry and the last inserted value wins.
pub fn merged_pairs(custom: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut map: HashMap<String, String> = DEFAULT_PAIRS
        .iter()
        .map(|(k, v)| ((*k).to_lowercase(), (*v).to_string()))
        .collect();
    for (k, v) in custom {
        map.insert(k.to_lowercase(), v.clone());
    }
    let mut out: Vec<(String, String)> = map.into_iter().collect();
    out.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    out
}
