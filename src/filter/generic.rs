#[allow(dead_code)]
pub const THRESHOLD_LINES: u32 = 500;

const HEAD_TAIL_LINES: usize = 20;
const HEAD_TAIL_BYTES: usize = 2048;

fn summarize_by_lines(lines: &[&str], line_count: usize) -> String {
    if line_count <= HEAD_TAIL_LINES * 2 {
        return lines.join("\n");
    }

    let head: Vec<&str> = lines.iter().take(HEAD_TAIL_LINES).copied().collect();
    let tail: Vec<&str> = lines.iter().rev().take(HEAD_TAIL_LINES).rev().copied().collect();
    let omitted = line_count.saturating_sub(HEAD_TAIL_LINES * 2);

    format!(
        "{}\n[ecotokens] ... {} lines omitted ({} total) ...\n{}",
        head.join("\n"),
        omitted,
        line_count,
        tail.join("\n"),
    )
}

/// Generic filter: passthrough if under threshold, head+tail summary otherwise.
pub fn filter_generic(output: &str, threshold_lines: u32, threshold_bytes: u32) -> String {
    let byte_len = output.len();
    let lines: Vec<&str> = output.lines().collect();
    let line_count = lines.len();

    let over_lines = line_count > threshold_lines as usize;
    let over_bytes = byte_len > threshold_bytes as usize;

    if !over_lines && !over_bytes {
        return output.to_string();
    }

    // When very few large lines trigger the byte threshold, truncate by bytes.
    if over_bytes && line_count <= HEAD_TAIL_LINES * 2 {
        let head = &output[..HEAD_TAIL_BYTES.min(byte_len / 2)];
        let tail_start = byte_len.saturating_sub(HEAD_TAIL_BYTES.min(byte_len / 2));
        let tail = &output[tail_start..];
        return format!(
            "{}\n[ecotokens] ... {} bytes omitted ({} total) ...\n{}",
            head,
            byte_len.saturating_sub(HEAD_TAIL_BYTES * 2),
            byte_len,
            tail,
        );
    }

    summarize_by_lines(&lines, line_count)
}

/// Force a generic summary, even when output is below normal thresholds.
pub fn force_filter_generic(output: &str) -> String {
    let byte_len = output.len();
    let lines: Vec<&str> = output.lines().collect();
    let line_count = lines.len();

    if line_count <= HEAD_TAIL_LINES * 2 && byte_len <= HEAD_TAIL_BYTES * 2 {
        return format!(
            "{}\n[ecotokens] passthrough disabled: output kept intact",
            output
        );
    }

    if line_count <= HEAD_TAIL_LINES * 2 && byte_len > HEAD_TAIL_BYTES * 2 {
        let head = &output[..HEAD_TAIL_BYTES.min(byte_len / 2)];
        let tail_start = byte_len.saturating_sub(HEAD_TAIL_BYTES.min(byte_len / 2));
        let tail = &output[tail_start..];
        return format!(
            "{}\n[ecotokens] ... {} bytes omitted ({} total) ...\n{}",
            head,
            byte_len.saturating_sub(HEAD_TAIL_BYTES * 2),
            byte_len,
            tail,
        );
    }

    summarize_by_lines(&lines, line_count)
}
