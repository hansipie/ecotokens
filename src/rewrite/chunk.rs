//! Structure-aware chunking for documents exceeding the model's context
//! window (FR-012 to FR-016, research.md §4-5, data-model.md `Chunk`).
//!
//! The single strongest invariant here (SC-009): concatenating every
//! [`Chunk::text`] in `index` order must reproduce the source **byte-for-byte**,
//! including blank-line structure. Every splitting function below is
//! "boundary-inclusive" — a separator (blank line, sentence-ending
//! punctuation + whitespace, or a whitespace run) is appended to the piece
//! that precedes it rather than dropped, so no byte of the source is ever
//! lost or duplicated by the segmentation itself.

use lazy_regex::regex;
use regex::Regex;

/// A bounded segment of a long input. `atomic` chunks (fenced code, tables,
/// list groups) are never split internally, even when they exceed the
/// budget — an oversized atomic chunk is simply passed through verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub index: usize,
    pub text: String,
    pub atomic: bool,
}

fn token_count(s: &str) -> u32 {
    crate::tokens::count_tokens(s) as u32
}

/// Effective per-chunk input budget: `context_tokens` minus a 25% reserve for
/// prompt overhead and response headroom (research.md §4). Default
/// `context_tokens` is 8192, giving an effective budget of ~6144.
pub fn effective_chunk_budget(context_tokens: u32) -> u32 {
    ((context_tokens as f64) * 0.75).floor().max(1.0) as u32
}

// ── A tiny internal segmentation unit, used only while building `Chunk`s ──

#[derive(Debug, Clone)]
struct Atom {
    text: String,
    atomic: bool,
}

/// Pass 1: carve out fenced code blocks (` ```...``` `) as atomic atoms;
/// everything between them is passed through for further segmentation.
fn segment_fenced(text: &str) -> Vec<Atom> {
    let re: &Regex = regex!(r"(?s)```.*?```");

    let mut atoms = Vec::new();
    let mut pos = 0;
    for m in re.find_iter(text) {
        if m.start() > pos {
            atoms.push(Atom {
                text: text[pos..m.start()].to_string(),
                atomic: false,
            });
        }
        atoms.push(Atom {
            text: text[m.start()..m.end()].to_string(),
            atomic: true,
        });
        pos = m.end();
    }
    if pos < text.len() {
        atoms.push(Atom {
            text: text[pos..].to_string(),
            atomic: false,
        });
    }
    if atoms.is_empty() {
        atoms.push(Atom {
            text: text.to_string(),
            atomic: false,
        });
    }
    atoms
}

fn is_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn is_list_line(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }
    let list_num_re = list_number_regex();
    list_num_re.is_match(t)
}

fn list_number_regex() -> &'static Regex {
    regex!(r"^\d+\.\s")
}

/// Pass 2: within non-fenced text, group contiguous table lines and
/// contiguous list-item lines into atomic runs; everything else stays
/// grouped as ordinary prose for paragraph segmentation.
fn segment_structures(text: &str) -> Vec<Atom> {
    if text.is_empty() {
        return Vec::new();
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut atoms = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let content = lines[i].trim_end_matches(['\n', '\r']);
        let is_table = is_table_line(content);
        let is_list = is_list_line(content);
        let start = i;
        if is_table {
            while i < lines.len() && is_table_line(lines[i].trim_end_matches(['\n', '\r'])) {
                i += 1;
            }
            atoms.push(Atom {
                text: lines[start..i].concat(),
                atomic: true,
            });
        } else if is_list {
            while i < lines.len() && is_list_line(lines[i].trim_end_matches(['\n', '\r'])) {
                i += 1;
            }
            atoms.push(Atom {
                text: lines[start..i].concat(),
                atomic: true,
            });
        } else {
            while i < lines.len() {
                let c = lines[i].trim_end_matches(['\n', '\r']);
                if is_table_line(c) || is_list_line(c) {
                    break;
                }
                i += 1;
            }
            atoms.push(Atom {
                text: lines[start..i].concat(),
                atomic: false,
            });
        }
    }
    atoms
}

/// Pass 3: split prose on blank-line paragraph boundaries. The boundary
/// (one or more blank lines) stays attached to the end of the preceding
/// paragraph, so re-concatenation is exact.
fn segment_paragraphs(text: &str) -> Vec<Atom> {
    if text.is_empty() {
        return Vec::new();
    }
    let re: &Regex = regex!(r"(?:\r?\n[ \t]*){2,}");
    let mut atoms = Vec::new();
    let mut pos = 0;
    for m in re.find_iter(text) {
        atoms.push(Atom {
            text: text[pos..m.end()].to_string(),
            atomic: false,
        });
        pos = m.end();
    }
    if pos < text.len() {
        atoms.push(Atom {
            text: text[pos..].to_string(),
            atomic: false,
        });
    }
    if atoms.is_empty() {
        atoms.push(Atom {
            text: text.to_string(),
            atomic: false,
        });
    }
    atoms
}

fn segment_all(text: &str) -> Vec<Atom> {
    let mut out = Vec::new();
    for fenced in segment_fenced(text) {
        if fenced.atomic {
            out.push(fenced);
            continue;
        }
        for structured in segment_structures(&fenced.text) {
            if structured.atomic {
                out.push(structured);
                continue;
            }
            for para in segment_paragraphs(&structured.text) {
                out.push(para);
            }
        }
    }
    out
}

/// Sentence tier: split on `.`/`!`/`?` followed by whitespace, boundary
/// inclusive (FR-013).
fn segment_sentences(text: &str) -> Vec<String> {
    let re: &Regex = regex!(r"[.!?]+[ \t]+");
    let mut out = Vec::new();
    let mut pos = 0;
    for m in re.find_iter(text) {
        out.push(text[pos..m.end()].to_string());
        pos = m.end();
    }
    if pos < text.len() {
        out.push(text[pos..].to_string());
    }
    if out.is_empty() {
        out.push(text.to_string());
    }
    out
}

/// Whitespace tier: greedily accumulate whitespace-delimited words under
/// `budget`, boundary inclusive. Last resort when even a single sentence
/// exceeds the budget (FR-013).
fn segment_whitespace(text: &str, budget: u32) -> Vec<String> {
    let re: &Regex = regex!(r"\s+");
    let mut out = Vec::new();
    let mut current = String::new();
    let mut current_tokens = 0u32;
    let mut piece_start = 0;

    for m in re.find_iter(text) {
        let piece = &text[piece_start..m.end()];
        let piece_tokens = token_count(piece);
        if current_tokens + piece_tokens > budget && !current.is_empty() {
            out.push(std::mem::take(&mut current));
            current_tokens = 0;
        }
        current.push_str(piece);
        current_tokens += piece_tokens;
        piece_start = m.end();
    }
    if piece_start < text.len() {
        current.push_str(&text[piece_start..]);
    }
    if !current.is_empty() {
        out.push(current);
    }
    if out.is_empty() {
        out.push(text.to_string());
    }
    out
}

/// A single paragraph atom exceeded the budget: fall back sentence tier,
/// then (if a sentence is itself still too large) whitespace tier. Records a
/// warning only when the whitespace tier is actually used.
fn split_oversized_prose(text: &str, budget: u32, warnings: &mut Vec<String>) -> Vec<Atom> {
    let mut out = Vec::new();
    for sentence in segment_sentences(text) {
        if token_count(&sentence) <= budget {
            out.push(Atom {
                text: sentence,
                atomic: false,
            });
        } else {
            warnings.push(format!(
                "a single sentence ({} tokens) exceeds the chunk budget ({budget} tokens); \
                falling back to a whitespace split",
                token_count(&sentence)
            ));
            for word_run in segment_whitespace(&sentence, budget) {
                out.push(Atom {
                    text: word_run,
                    atomic: false,
                });
            }
        }
    }
    out
}

/// Split `text` into chunks, none exceeding `budget_tokens` unless the piece
/// is atomic (or, in the rare case of a single unsplittable oversized token,
/// unavoidably so). Returns the chunks plus any boundary-fallback warnings
/// (FR-012, FR-013). Concatenating every `Chunk::text` in order reproduces
/// `text` byte-for-byte (SC-009) — this is `split_into_chunks`'s core
/// contract and is asserted by `tests/rewrite/chunk_test.rs`.
pub fn split_into_chunks(text: &str, budget_tokens: u32) -> (Vec<Chunk>, Vec<String>) {
    if text.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut warnings = Vec::new();
    let atoms = segment_all(text);

    let mut pieces: Vec<Atom> = Vec::new();
    for atom in atoms {
        if atom.atomic || token_count(&atom.text) <= budget_tokens {
            pieces.push(atom);
        } else {
            pieces.extend(split_oversized_prose(
                &atom.text,
                budget_tokens,
                &mut warnings,
            ));
        }
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut current = String::new();
    let mut current_tokens = 0u32;

    for piece in pieces {
        let piece_tokens = token_count(&piece.text);

        if piece.atomic || piece_tokens > budget_tokens {
            if !current.is_empty() {
                chunks.push(Chunk {
                    index: chunks.len(),
                    text: std::mem::take(&mut current),
                    atomic: false,
                });
                current_tokens = 0;
            }
            chunks.push(Chunk {
                index: chunks.len(),
                text: piece.text,
                atomic: piece.atomic,
            });
            continue;
        }

        if current_tokens + piece_tokens > budget_tokens && !current.is_empty() {
            chunks.push(Chunk {
                index: chunks.len(),
                text: std::mem::take(&mut current),
                atomic: false,
            });
            current_tokens = 0;
        }
        current.push_str(&piece.text);
        current_tokens += piece_tokens;
    }
    if !current.is_empty() {
        chunks.push(Chunk {
            index: chunks.len(),
            text: current,
            atomic: false,
        });
    }

    (chunks, warnings)
}

/// Reassembly is a straight concatenation: every chunk produced by
/// `split_into_chunks` already carries its own trailing separator, so no
/// extra glue is needed to preserve blank-line structure (FR-014).
pub fn reassemble(pieces: &[String]) -> String {
    pieces.concat()
}

const ANCHOR_TOKEN_BUDGET: u32 = 200;

/// Build a cross-chunk style anchor from the tail of a chunk's *transformed*
/// output — the last ~200 tokens, whitespace-aligned (research.md §5).
pub fn carry_over_anchor(transformed: &str) -> String {
    if token_count(transformed) <= ANCHOR_TOKEN_BUDGET {
        return transformed.to_string();
    }
    let words: Vec<&str> = transformed.split_whitespace().collect();
    let mut acc: Vec<&str> = Vec::new();
    let mut tokens = 0u32;
    for w in words.iter().rev() {
        let wt = token_count(w);
        if tokens + wt > ANCHOR_TOKEN_BUDGET && !acc.is_empty() {
            break;
        }
        acc.push(w);
        tokens += wt;
    }
    acc.reverse();
    acc.join(" ")
}

/// If the model echoed the (non-emitted) anchor verbatim at the start of its
/// output despite being told not to, strip it before treating the rest as
/// the chunk's real transformed text (FR-016).
pub fn strip_echoed_anchor(output: &str, anchor: &str) -> String {
    let anchor_trimmed = anchor.trim();
    if anchor_trimmed.is_empty() {
        return output.to_string();
    }
    let trimmed = output.trim_start();
    if let Some(rest) = trimmed.strip_prefix(anchor_trimmed) {
        return rest.trim_start().to_string();
    }
    output.to_string()
}
