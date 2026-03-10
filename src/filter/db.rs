const MAX_DATA_ROWS: usize = 30;
const MAX_EXPANDED_RECORDS: usize = 20;

/// Filter psql output (table or expanded format).
pub fn filter_db(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if is_table_format(&lines) {
        filter_table(output)
    } else if is_expanded_format(&lines) {
        filter_expanded(output)
    } else {
        output.to_string()
    }
}

fn is_table_format(lines: &[&str]) -> bool {
    lines
        .iter()
        .any(|l| l.contains('─') || l.contains("-+-") || l.contains("+--+"))
}

fn is_expanded_format(lines: &[&str]) -> bool {
    lines.iter().any(|l| l.contains("-[ RECORD"))
}

fn filter_table(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut data_rows: Vec<String> = Vec::new();
    let mut header: Option<String> = None;

    for line in &lines {
        if is_separator(line) {
            continue;
        }
        // Skip footer like "(N rows)"
        let t = line.trim();
        if t.starts_with('(') && t.ends_with(')') && t.contains("row") {
            continue;
        }
        if header.is_none() {
            header = Some(to_tab_separated(line));
        } else {
            data_rows.push(to_tab_separated(line));
        }
    }

    let total = data_rows.len();
    let shown = total.min(MAX_DATA_ROWS);
    let mut result = Vec::new();

    if let Some(h) = header {
        result.push(h);
    }
    for row in &data_rows[..shown] {
        result.push(row.clone());
    }
    if total > MAX_DATA_ROWS {
        result.push(format!(
            "[ecotokens] ... {} more rows omitted ...",
            total - MAX_DATA_ROWS
        ));
    }

    result.join("\n")
}

fn is_separator(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    t.chars()
        .all(|c| matches!(c, '─' | '-' | '+' | '|' | '┼' | '┤' | '├' | ' '))
        && (t.contains('─') || t.contains("-+-") || t.contains("+--+") || t.contains("---"))
}

fn to_tab_separated(line: &str) -> String {
    line.split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\t")
}

fn filter_expanded(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in &lines {
        if line.contains("-[ RECORD") {
            if !current.is_empty() {
                records.push(current.clone());
                current.clear();
            }
        } else if line.contains(" | ") {
            let parts: Vec<&str> = line.splitn(2, " | ").collect();
            if parts.len() == 2 {
                current.push(format!("{} = {}", parts[0].trim(), parts[1].trim()));
            }
        }
    }
    if !current.is_empty() {
        records.push(current);
    }

    let total = records.len();
    let shown = total.min(MAX_EXPANDED_RECORDS);
    let mut result = Vec::new();

    for (i, record) in records[..shown].iter().enumerate() {
        result.push(format!("-- record {} --", i + 1));
        result.extend(record.iter().cloned());
    }
    if total > MAX_EXPANDED_RECORDS {
        result.push(format!(
            "[ecotokens] ... {} more records omitted ...",
            total - MAX_EXPANDED_RECORDS
        ));
    }

    result.join("\n")
}
