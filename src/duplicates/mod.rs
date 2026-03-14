pub mod detect;
pub mod proposals;
pub mod staleness;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSegment {
    pub symbol_id: String,
    pub file_path: String,
    pub line_start: u64,
    pub line_end: u64,
    pub content: String,
}

impl CodeSegment {
    #[allow(dead_code)]
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityScore {
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    ExactDuplicate,
    NearDuplicate,
    SubsetOf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringProposal {
    pub kind: ProposalKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub id: usize,
    pub similarity: SimilarityScore,
    pub segments: Vec<CodeSegment>,
    pub proposals: Vec<RefactoringProposal>,
}

#[derive(Debug, Clone)]
pub struct DetectionOptions {
    pub index_dir: PathBuf,
    pub threshold: f32,
    pub min_lines: usize,
}
