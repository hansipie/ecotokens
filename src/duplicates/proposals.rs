use super::{CodeSegment, DuplicateGroup, ProposalKind, RefactoringProposal};

pub fn generate_proposals(segments: &[CodeSegment], similarity: f32) -> Vec<RefactoringProposal> {
    if segments.len() < 2 {
        return vec![];
    }

    // Every member of the group, so proposals cover 3+ members instead of only
    // the first two.
    let locations: Vec<String> = segments
        .iter()
        .map(|s| format!("{}:{}", s.file_path, s.line_start))
        .collect();
    let all = locations.join(", ");

    // Check exact duplicate (100% similarity)
    if (similarity - 100.0).abs() < f32::EPSILON {
        return vec![RefactoringProposal {
            kind: ProposalKind::ExactDuplicate,
            text: format!(
                "Exact duplicate detected across {} locations: {all}. \
                Consider extracting to a shared function to eliminate redundancy.",
                segments.len()
            ),
        }];
    }

    // Check subset relationships across all ordered pairs.
    let mut proposals = Vec::new();
    for i in 0..segments.len() {
        for j in 0..segments.len() {
            if i == j {
                continue;
            }
            if is_subset(&segments[i].content, &segments[j].content) {
                proposals.push(RefactoringProposal {
                    kind: ProposalKind::SubsetOf,
                    text: format!(
                        "{}:{} appears to be a subset of {}:{}. \
                        Consider refactoring to reuse the larger implementation.",
                        segments[i].file_path,
                        segments[i].line_start,
                        segments[j].file_path,
                        segments[j].line_start
                    ),
                });
            }
        }
    }
    if !proposals.is_empty() {
        return proposals;
    }

    // Near duplicate
    vec![RefactoringProposal {
        kind: ProposalKind::NearDuplicate,
        text: format!(
            "Near-duplicate code ({similarity:.1}% similar) found across {} locations: {all}. \
            Consider extracting common logic into a shared abstraction.",
            segments.len()
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

pub fn format_duplicates_plain(
    groups: &[DuplicateGroup],
    threshold: f32,
    min_lines: usize,
) -> String {
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
