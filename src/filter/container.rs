use crate::filter::generic::filter_generic;

/// Filter container tooling output (docker, podman, kubectl).
pub fn filter_container(command: &str, output: &str) -> String {
    let cmd = command.trim().to_lowercase();

    if cmd.contains(" ps") && (cmd.starts_with("docker") || cmd.starts_with("podman")) {
        filter_container_ps(output)
    } else if cmd.contains(" logs") && (cmd.starts_with("docker") || cmd.starts_with("podman")) {
        filter_container_logs(output)
    } else if cmd.contains(" images") && (cmd.starts_with("docker") || cmd.starts_with("podman")) {
        filter_docker_images(output)
    } else if cmd.starts_with("kubectl get") {
        filter_kubectl_get(output)
    } else if cmd.starts_with("kubectl logs") {
        filter_container_logs(output)
    } else {
        filter_generic(output, 500, 51200)
    }
}

fn filter_container_ps(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 20 {
        return output.to_string();
    }

    // docker/podman ps columns: CONTAINER_ID, IMAGE, COMMAND, CREATED, STATUS, PORTS, NAMES
    // We keep ID, Name, Status, Image, Ports in compact form
    let mut result = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            // Header line — emit compact header
            result.push(format!(
                "{:<15} {:<30} {:<20} {:<15}",
                "ID", "NAME", "STATUS", "IMAGE"
            ));
            continue;
        }

        if line.is_empty() {
            continue;
        }

        // Split by multiple spaces (docker ps uses wide spacing)
        let cols: Vec<&str> = line
            .splitn(7, "   ")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if cols.len() >= 2 {
            let id = cols.first().unwrap_or(&"");
            // Truncate to short ID
            let short_id = if id.len() > 12 { &id[..12] } else { id };

            // Find relevant columns from the header positions
            let name = cols.last().unwrap_or(&"");
            let status = if cols.len() > 4 {
                cols[4]
            } else if cols.len() > 3 {
                cols[3]
            } else {
                ""
            };
            let image = if cols.len() > 1 { cols[1] } else { "" };
            let ports = if cols.len() > 5 { cols[5] } else { "" };

            if ports.is_empty() {
                result.push(format!(
                    "{:<15} {:<30} {:<20} {:<15}",
                    short_id, name, status, image
                ));
            } else {
                result.push(format!(
                    "{:<15} {:<30} {:<20} {:<15} {}",
                    short_id, name, status, image, ports
                ));
            }
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

fn filter_container_logs(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 50 {
        return output.to_string();
    }

    // Deduplicate consecutive identical lines with counting
    let mut result: Vec<String> = Vec::new();
    let mut last_line = "";
    let mut count = 0usize;

    for line in &lines {
        if *line == last_line {
            count += 1;
        } else {
            if count > 1 {
                result.push(format!("[{}x] {}", count, last_line));
            } else if count == 1 {
                result.push(last_line.to_string());
            }
            last_line = line;
            count = 1;
        }
    }
    // Push last group
    if count > 1 {
        result.push(format!("[{}x] {}", count, last_line));
    } else if count == 1 && !last_line.is_empty() {
        result.push(last_line.to_string());
    }

    if result.len() < lines.len() {
        result.join("\n")
    } else {
        filter_generic(output, 200, 51200)
    }
}

fn filter_kubectl_get(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 30 {
        return output.to_string();
    }

    // kubectl get pods/services already uses tabular format — keep as-is but truncate
    // Keep header + all lines, but limit to 100 entries
    const MAX_ROWS: usize = 100;
    if lines.len() > MAX_ROWS + 1 {
        let mut result: Vec<String> = lines
            .iter()
            .take(MAX_ROWS + 1)
            .map(|s| s.to_string())
            .collect();
        let omitted = lines.len() - MAX_ROWS - 1;
        result.push(format!("[ecotokens] ... {} more rows omitted ...", omitted));
        result.join("\n")
    } else {
        output.to_string()
    }
}

/// Filter `docker/podman images` output: compact REPOSITORY:TAG SIZE format.
fn filter_docker_images(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= 5 {
        return output.to_string();
    }

    let mut result = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            // Skip header line
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        // docker images columns: REPOSITORY  TAG  IMAGE ID  CREATED  SIZE
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 5 {
            let repo = cols[0];
            let tag = cols[1];
            let size = cols[cols.len() - 1];
            result.push(format!("{}:{}  {}", repo, tag, size));
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}
