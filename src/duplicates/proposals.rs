use super::{CodeSegment, DuplicateGroup, ProposalKind, RefactoringProposal};

pub fn generate_proposals(segments: &[CodeSegment], similarity: f32) -> Vec<RefactoringProposal> {
    if segments.len() < 2 {
        return vec![];
    }

    let a = &segments[0];
    let b = &segments[1];

    // Check exact duplicate (100% similarity)
    if (similarity - 100.0).abs() < f32::EPSILON {
        return vec![RefactoringProposal {
            kind: ProposalKind::ExactDuplicate,
            text: format!(
                "Exact duplicate detected: {}:{} and {}:{}. \
                Consider extracting to a shared function to eliminate redundancy.",
                a.file_path, a.line_start, b.file_path, b.line_start
            ),
        }];
    }

    // Check subset relationship
    if is_subset(&a.content, &b.content) {
        return vec![RefactoringProposal {
            kind: ProposalKind::SubsetOf,
            text: format!(
                "{}:{} appears to be a subset of {}:{}. \
                Consider refactoring to reuse the larger implementation.",
                a.file_path, a.line_start, b.file_path, b.line_start
            ),
        }];
    }
    if is_subset(&b.content, &a.content) {
        return vec![RefactoringProposal {
            kind: ProposalKind::SubsetOf,
            text: format!(
                "{}:{} appears to be a subset of {}:{}. \
                Consider refactoring to reuse the larger implementation.",
                b.file_path, b.line_start, a.file_path, a.line_start
            ),
        }];
    }

    // Near duplicate
    vec![RefactoringProposal {
        kind: ProposalKind::NearDuplicate,
        text: format!(
            "Near-duplicate code ({:.1}% similar) found at {}:{} and {}:{}. \
            Consider extracting common logic into a shared abstraction.",
            similarity, a.file_path, a.line_start, b.file_path, b.line_start
        ),
    }]
}

fn is_subset(shorter: &str, longer: &str) -> bool {
    let shorter_trimmed = shorter.trim();
    let longer_trimmed = longer.trim();
    if shorter_trimmed.is_empty() || longer_trimmed.is_empty() {
        return false;
    }
    if shorter_trimmed.len() >= longer_trimmed.len() {
        return false;
    }
    longer_trimmed.contains(shorter_trimmed)
}

pub fn format_duplicates_plain(groups: &[DuplicateGroup], threshold: f32, min_lines: usize) -> String {
    if groups.is_empty() {
        return format!(
            "No duplicates found (threshold: {threshold:.0}%, min_lines: {min_lines}).\n"
        );
    }

    let mut out = String::new();
    out.push_str(&format!(
        "Found {} duplicate group(s) (threshold: {threshold:.0}%, min_lines: {min_lines})\n\n",
        groups.len()
    ));

    for group in groups {
        out.push_str(&format!(
            "── Group {} ({:.1}% similar) ──\n",
            group.id, group.similarity.value
        ));
        for seg in &group.segments {
            out.push_str(&format!(
                "  {}  lines {}-{}\n",
                seg.file_path, seg.line_start, seg.line_end
            ));
        }
        for proposal in &group.proposals {
            out.push_str(&format!("  → {}\n", proposal.text));
        }
        out.push('\n');
    }

    out
}
