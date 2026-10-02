const HEAD_TAIL_LINES: usize = 20;
const HEAD_TAIL_BYTES: usize = 2048;

/// Largest byte index ≤ `idx` that falls on a UTF-8 char boundary.
pub(crate) fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn ceil_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx < s.len() && !s.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn summarize_by_lines(lines: &[&str], line_count: usize) -> String {
    if line_count <= HEAD_TAIL_LINES * 2 {
        return lines.join("\n");
    }

    let head: Vec<&str> = lines.iter().take(HEAD_TAIL_LINES).copied().collect();
    let tail: Vec<&str> = lines
        .iter()
        .rev()
        .take(HEAD_TAIL_LINES)
        .rev()
        .copied()
        .collect();
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
        let slice_len = HEAD_TAIL_BYTES.min(byte_len / 2);
        let head_end = floor_char_boundary(output, slice_len);
        let tail_start = ceil_char_boundary(output, byte_len.saturating_sub(slice_len));
        let head = &output[..head_end];
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
#[allow(dead_code)]
pub fn force_filter_generic(output: &str) -> String {
    // Always reduce filtered output by at least one estimated token (4 chars).
    let chars: Vec<char> = output.chars().collect();
    if chars.is_empty() {
        return String::new();
    }

    let total = chars.len();
    let keep = total.saturating_sub(4);
    if keep == 0 {
        return String::new();
    }

    let head_len = keep / 2;
    let tail_len = keep - head_len;

    let mut reduced = String::with_capacity(keep);
    reduced.extend(chars.iter().take(head_len));
    reduced.extend(chars.iter().skip(total - tail_len));
    reduced
}

// ── Jev line selection (optional; `filter_generic` stays the fallback) ─────

const JEV_WINDOW_LINES: usize = 200;
const JEV_MAX_WINDOWS: usize = 10;
const JEV_MAX_LINE_CHARS: usize = 200;
const JEV_MAX_KEPT_PER_WINDOW: usize = 10;
const JEV_ANY_MIN_PROB: f64 = 0.5;

fn line_id(idx: usize) -> String {
    format!("L{idx:05}")
}

fn clip_line(line: &str) -> String {
    let end = floor_char_boundary(line, JEV_MAX_LINE_CHARS);
    if end < line.len() {
        format!("{}…", &line[..end])
    } else {
        line.to_string()
    }
}

/// [`filter_generic`] where, in the line-count case, Jev picks the lines
/// between head and tail that report failures, errors, or identifiers a
/// developer needs (line-by-line search pattern: one Choice over line ids
/// plus one "any failure here?" Noul per window, all in one request).
///
/// Falls back to [`filter_generic`] unchanged when the output is under the
/// thresholds, takes the byte-truncation path, spans more than
/// `JEV_MAX_WINDOWS` windows or `jev_max_input_chars`, when the call fails
/// or an answer is missing, or when Jev keeps no line at all.
pub fn filter_generic_with_judge(
    output: &str,
    threshold_lines: u32,
    threshold_bytes: u32,
    ctx: crate::jev::JevContext<'_>,
) -> String {
    let fallback = || filter_generic(output, threshold_lines, threshold_bytes);

    let lines: Vec<&str> = output.lines().collect();
    let line_count = lines.len();
    let over_lines = line_count > threshold_lines as usize;
    let over_bytes = output.len() > threshold_bytes as usize;
    if (!over_lines && !over_bytes) || line_count <= HEAD_TAIL_LINES * 2 {
        return fallback();
    }

    let middle_start = HEAD_TAIL_LINES;
    let middle_end = line_count - HEAD_TAIL_LINES;
    let middle: Vec<usize> = (middle_start..middle_end).collect();
    let windows: Vec<&[usize]> = middle.chunks(JEV_WINDOW_LINES).collect();
    if windows.len() > JEV_MAX_WINDOWS {
        return fallback();
    }

    let mut state = serde_json::Map::new();
    let mut questions = crate::jev::Questions::new();
    let mut total_chars = 0usize;
    for (k, window) in windows.iter().enumerate() {
        let text: String = window
            .iter()
            .map(|&i| format!("{} {}", line_id(i), clip_line(lines[i])))
            .collect::<Vec<_>>()
            .join("\n");
        total_chars += text.chars().count();
        let key = format!("window_{k}");
        questions.insert(
            format!("pick_{k}"),
            crate::jev::Question::choice(
                format!(
                    "Which line of `{key}` reports a failure, error, warning, or an \
                     identifier (file, test, id) a developer needs to act on this output?"
                ),
                window
                    .iter()
                    .map(|&i| (line_id(i), format!("Line {}", line_id(i)))),
            ),
        );
        questions.insert(
            format!("any_{k}"),
            crate::jev::Question::noul_with(
                format!("Does any line of `{key}` report a failure, error, or warning?"),
                "At least one line reports something that went wrong or needs attention",
                "The lines are routine progress, success, or informational output",
            ),
        );
        state.insert(key, serde_json::Value::String(text));
    }
    if total_chars > ctx.settings.jev_max_input_chars {
        return fallback();
    }

    let Ok(answers) = ctx.ask_for(
        crate::jev::stats::Purpose::FilterLines,
        serde_json::Value::Object(state),
        questions,
        ctx.timeout(None),
    ) else {
        return fallback();
    };

    let mut keep = std::collections::BTreeSet::new();
    for (k, window) in windows.iter().enumerate() {
        let (Some(any), Some(pick)) = (
            answers.noul(&format!("any_{k}")),
            answers.choice(&format!("pick_{k}")),
        ) else {
            return fallback();
        };
        if any < JEV_ANY_MIN_PROB {
            continue;
        }
        let mut ranked: Vec<(usize, f64)> = window
            .iter()
            .map(|&i| (i, pick.prob(&line_id(i))))
            .filter(|(_, p)| *p >= ctx.settings.jev_line_keep_min_prob)
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        keep.extend(
            ranked
                .into_iter()
                .take(JEV_MAX_KEPT_PER_WINDOW)
                .map(|(i, _)| i),
        );
    }
    if keep.is_empty() {
        return fallback();
    }

    let mut out: Vec<String> = lines[..middle_start]
        .iter()
        .map(|l| l.to_string())
        .collect();
    let mut gap = 0usize;
    let mut first_marker = true;
    let mut flush_gap = |out: &mut Vec<String>, gap: &mut usize| {
        if *gap > 0 {
            if first_marker {
                out.push(format!(
                    "[ecotokens] ... {gap} lines omitted ({line_count} total, {} kept by Jev) ...",
                    keep.len()
                ));
                first_marker = false;
            } else {
                out.push(format!("[ecotokens] ... {gap} lines omitted ..."));
            }
            *gap = 0;
        }
    };
    for (i, line) in lines.iter().enumerate().take(middle_end).skip(middle_start) {
        if keep.contains(&i) {
            flush_gap(&mut out, &mut gap);
            out.push(line.to_string());
        } else {
            gap += 1;
        }
    }
    flush_gap(&mut out, &mut gap);
    out.extend(lines[middle_end..].iter().map(|l| l.to_string()));
    out.join("\n")
}
