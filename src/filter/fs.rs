use crate::filter::generic::filter_generic;

const FS_LINE_THRESHOLD: u32 = 100;

/// Filter filesystem command output (ls, find, tree).
pub fn filter_fs(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();
    let threshold = if cmd.starts_with("find") || cmd.starts_with("tree") { 500 } else { FS_LINE_THRESHOLD };
    filter_generic(output, threshold, 51200)
}
