use crate::filter::generic::filter_generic;
use lazy_regex::regex;

#[allow(dead_code)]
const MD_LINE_THRESHOLD: usize = 200;

/// Filter Markdown content: passthrough if short; ToC + first section if long.
pub fn filter_markdown(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() <= MD_LINE_THRESHOLD {
        return content.to_string();
    }

    // Extract headings H1–H3
    let heading_re = regex!(r"^(#{1,3})\s+(.+)");
    let headings: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| heading_re.captures(line).map(|_| (i, *line)))
        .collect();

    if headings.is_empty() {
        return filter_generic(content, MD_LINE_THRESHOLD as u32, 51200);
    }

    // Build ToC
    let toc: Vec<String> = headings
        .iter()
        .map(|(_, heading)| format!("  {heading}"))
        .collect();

    // First section: from first heading to second heading (or 50 lines max)
    let first_section_start = headings[0].0;
    let first_section_end = if headings.len() > 1 {
        headings[1].0.min(first_section_start + 50)
    } else {
        (first_section_start + 50).min(lines.len())
    };

    let section_lines = &lines[first_section_start..first_section_end];
    let remaining_sections = headings.len().saturating_sub(1);

    format!(
        "[ecotokens] Markdown summary ({} lines total)\n\nSections:\n{}\n\n{}\n\n[ecotokens] ... {} more sections omitted ...",
        lines.len(),
        toc.join("\n"),
        section_lines.join("\n"),
        remaining_sections,
    )
}
