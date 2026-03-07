use crate::filter::generic::filter_generic;
use lazy_regex::regex;

const WARNING_THRESHOLD: usize = 10;
const WARNING_SAMPLES: usize = 3;

/// Filter C/C++ toolchain output.
pub fn filter_cpp(_command: &str, output: &str) -> String {
    filter_compiler_output(output)
}

fn filter_compiler_output(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    let error_re = regex!(r"(^|:\s)(fatal )?error:");
    let warning_re = regex!(r"(^|:\s)warning:");
    let summary_re = regex!(
        r"^(\d+ (warnings?|errors?) generated\.|collect2: error:|clang(\+\+)?(:|-\d+:) error:|g\+\+: error:|gcc: error:|cc1: all warnings being treated as errors|ninja: build stopped:|make(\[\d+\])?: \*\*\*|CMake Error|ld: error:)"
    );
    let success_re = regex!(r"(?i)(build succeeded|build completed|linking|finished)");

    let mut result: Vec<String> = Vec::new();
    let mut warning_count = 0;
    let mut kept_warning_samples = 0;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if error_re.is_match(line) {
            result.push(line.to_string());
            i += 1;

            while i < lines.len() {
                let next = lines[i];
                if next.trim().is_empty() {
                    result.push(next.to_string());
                    i += 1;
                    break;
                }
                if next.starts_with(' ') || next.contains(" |") || next.trim_start().starts_with('^')
                {
                    result.push(next.to_string());
                    i += 1;
                    continue;
                }
                break;
            }
            continue;
        }

        if warning_re.is_match(line) {
            warning_count += 1;
            let capture = kept_warning_samples < WARNING_SAMPLES;
            if capture {
                result.push(line.to_string());
                kept_warning_samples += 1;
            }
            i += 1;
            // Capture the diagnostic context block (source line + caret pointer) for sample warnings
            while capture && i < lines.len() {
                let next = lines[i];
                if next.trim().is_empty() {
                    result.push(next.to_string());
                    i += 1;
                    break;
                }
                if next.starts_with(' ') || next.contains(" |") || next.trim_start().starts_with('^') {
                    result.push(next.to_string());
                    i += 1;
                    continue;
                }
                break;
            }
            continue;
        }

        if summary_re.is_match(line) || success_re.is_match(line) {
            result.push(line.to_string());
        }

        i += 1;
    }

    if result.is_empty() {
        return filter_generic(output, 200, 51200);
    }

    // Only success/summary lines were captured, no real diagnostics — let generic handle it
    if warning_count == 0 && !result.iter().any(|l| error_re.is_match(l)) {
        return filter_generic(output, 200, 51200);
    }

    if warning_count <= WARNING_THRESHOLD && !result.iter().any(|line| error_re.is_match(line)) {
        return output.to_string();
    }

    if warning_count > WARNING_THRESHOLD {
        result.insert(
            kept_warning_samples.min(result.len()),
            format!("[ecotokens] {warning_count} compiler warnings summarized"),
        );
    }

    let filtered = result.join("\n");
    if filtered.len() < output.len() {
        filtered
    } else {
        filter_generic(output, 200, 51200)
    }
}
