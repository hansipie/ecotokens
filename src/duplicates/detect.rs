use similar::TextDiff;
use std::fmt;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use super::{CodeSegment, DetectionOptions, DuplicateGroup, SimilarityScore};
use crate::duplicates::proposals::generate_proposals;
use crate::search::index::build_schema;

#[derive(Debug)]
pub enum DetectError {
    IndexNotFound,
    Tantivy(tantivy::TantivyError),
}

impl fmt::Display for DetectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetectError::IndexNotFound => write!(f, "index not found; run `ecotokens index` first"),
            DetectError::Tantivy(e) => write!(f, "tantivy error: {e}"),
        }
    }
}

impl std::error::Error for DetectError {}

impl From<tantivy::TantivyError> for DetectError {
    fn from(e: tantivy::TantivyError) -> Self {
        DetectError::Tantivy(e)
    }
}

pub fn detect_duplicates(opts: &DetectionOptions) -> Result<Vec<DuplicateGroup>, DetectError> {
    // 1. Open index
    let index = Index::open_in_dir(&opts.index_dir).map_err(|_| DetectError::IndexNotFound)?;

    let (_, file_path_field, content_field, kind_field, line_start_field, symbol_id_field) =
        build_schema();

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;
    let searcher = reader.searcher();

    // 2. Query all kind=="symbol" docs
    let kind_term = Term::from_field_text(kind_field, "symbol");
    let kind_query = TermQuery::new(kind_term, IndexRecordOption::Basic);
    let top_docs = searcher.search(&kind_query, &TopDocs::with_limit(10_000))?;

    // 3. Build Vec<CodeSegment>, filter by min_lines
    let mut segments: Vec<CodeSegment> = Vec::new();
    for (_score, addr) in top_docs {
        let doc: TantivyDocument = searcher.doc(addr)?;

        let file_path = doc
            .get_first(file_path_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let content = doc
            .get_first(content_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let symbol_id = doc
            .get_first(symbol_id_field)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let line_start = doc
            .get_first(line_start_field)
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let line_count = content.lines().count();
        if line_count < opts.min_lines {
            continue;
        }

        let line_end = line_start + line_count as u64 - 1;

        segments.push(CodeSegment {
            symbol_id,
            file_path,
            line_start,
            line_end,
            content,
        });
    }

    let n = segments.len();
    if n < 2 {
        return Ok(vec![]);
    }

    // 4. Pairwise similarity + Union-Find
    let mut parent: Vec<usize> = (0..n).collect();
    let mut best_score: Vec<f32> = vec![0.0; n];

    for i in 0..n {
        for j in (i + 1)..n {
            let ratio =
                TextDiff::from_lines(&segments[i].content, &segments[j].content).ratio() * 100.0;
            if ratio >= opts.threshold {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    union(&mut parent, i, j);
                }
                // Track best score for each root
                let root = find(&mut parent, i);
                if ratio > best_score[root] {
                    best_score[root] = ratio;
                }
            }
        }
    }

    // 5. Group by root — collect all indices, then filter singletons
    let mut group_map: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        group_map.entry(root).or_default().push(i);
    }

    // 6. Build DuplicateGroup list, filter out singletons
    let mut groups: Vec<DuplicateGroup> = group_map
        .into_iter()
        .filter(|(_, idxs)| idxs.len() >= 2)
        .map(|(_, idxs)| {
            // Recalculate best score within this group
            let mut best = 0.0f32;
            for ii in 0..idxs.len() {
                for jj in (ii + 1)..idxs.len() {
                    let r = TextDiff::from_lines(
                        &segments[idxs[ii]].content,
                        &segments[idxs[jj]].content,
                    )
                    .ratio()
                        * 100.0;
                    if r > best {
                        best = r;
                    }
                }
            }
            let segs: Vec<CodeSegment> = idxs.iter().map(|&i| segments[i].clone()).collect();
            let proposals = generate_proposals(&segs, best);
            DuplicateGroup {
                id: 0, // assigned below
                similarity: SimilarityScore { value: best },
                segments: segs,
                proposals,
            }
        })
        .collect();

    // 7. Sort by similarity descending, assign ids 1..n
    groups.sort_by(|a, b| {
        b.similarity
            .value
            .partial_cmp(&a.similarity.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, g) in groups.iter_mut().enumerate() {
        g.id = i + 1;
    }

    Ok(groups)
}

fn find(parent: &mut Vec<usize>, i: usize) -> usize {
    if parent[i] != i {
        parent[i] = find(parent, parent[i]);
    }
    parent[i]
}

fn union(parent: &mut Vec<usize>, i: usize, j: usize) {
    let ri = find(parent, i);
    let rj = find(parent, j);
    if ri != rj {
        parent[rj] = ri;
    }
}
