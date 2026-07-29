use similar::TextDiff;
use std::fmt;
use tantivy::collector::TopDocs;
use tantivy::query::TermQuery;
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{Index, ReloadPolicy, TantivyDocument, Term};

use super::{CodeSegment, DetectionOptions, DuplicateGroup, SimilarityScore};
use crate::duplicates::proposals::generate_proposals;
use crate::search::index::build_schema;

const MAX_SYMBOL_DOCS: usize = 10_000;

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
    let top_docs = searcher.search(&kind_query, &TopDocs::with_limit(MAX_SYMBOL_DOCS))?;
    if top_docs.len() > MAX_SYMBOL_DOCS {
        eprintln!(
            "ecotokens: warning: symbol limit ({MAX_SYMBOL_DOCS}) reached; duplicate detection may be incomplete"
        );
    }

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
        if let Some(root) = &opts.project_root {
            if !root.join(&file_path).exists() {
                continue;
            }
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
    if n > 500 {
        eprintln!(
            "ecotokens: warning: comparing {n} symbols (~{} pairs); this may take a while",
            n * (n - 1) / 2
        );
    }
    let mut parent: Vec<usize> = (0..n).collect();
    let mut size: Vec<usize> = vec![1; n];
    let mut best_score: Vec<f32> = vec![0.0; n];

    // Pre-computed line counts to cheaply skip pairs that provably cannot reach
    // the threshold, avoiding the expensive TextDiff for those pairs.
    let line_counts: Vec<usize> = segments.iter().map(|s| s.content.lines().count()).collect();

    for i in 0..n {
        for j in (i + 1)..n {
            // Upper bound on the similarity ratio: matched lines cannot exceed the
            // shorter segment, so ratio ≤ 200 * min(li, lj) / (li + lj).
            let (li, lj) = (line_counts[i], line_counts[j]);
            let max_possible = if li + lj == 0 {
                0.0
            } else {
                200.0 * li.min(lj) as f32 / (li + lj) as f32
            };
            if max_possible < opts.threshold {
                continue;
            }

            let ratio =
                TextDiff::from_lines(&segments[i].content, &segments[j].content).ratio() * 100.0;
            if ratio >= opts.threshold {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    union(&mut parent, &mut size, i, j);
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

/// Union-find root lookup with iterative path compression — iterative so a long
/// parent chain cannot overflow the stack on large inputs.
fn find(parent: &mut [usize], i: usize) -> usize {
    let mut root = i;
    while parent[root] != root {
        root = parent[root];
    }
    // Point every node on the path directly at the root.
    let mut cur = i;
    while parent[cur] != root {
        let next = parent[cur];
        parent[cur] = root;
        cur = next;
    }
    root
}

/// Union by size: attach the smaller tree under the larger root to bound tree
/// depth (and therefore the work done by `find`) to O(log n).
fn union(parent: &mut [usize], size: &mut [usize], i: usize, j: usize) {
    let ri = find(parent, i);
    let rj = find(parent, j);
    if ri == rj {
        return;
    }
    let (large, small) = if size[ri] >= size[rj] {
        (ri, rj)
    } else {
        (rj, ri)
    };
    parent[small] = large;
    size[large] += size[small];
}
