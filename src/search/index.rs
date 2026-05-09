use chrono;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter, Term};

use super::embed::embed_text;
use super::hnsw::{HnswIndex, HnswMeta};
use super::is_indexable_extension;
use super::symbols::{parse_symbols, write_symbols};
use super::text_docs::index_text_doc;

#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub reset: bool,
    pub path: PathBuf,
    pub index_dir: PathBuf,
    pub progress: Option<Arc<AtomicUsize>>,
    /// Embedding provider (None = BM25 only)
    pub embed_provider: crate::config::settings::EmbedProvider,
    /// Optional channel to forward log messages (used by TUI watch to avoid breaking the screen).
    /// When None, messages are printed to stderr directly.
    pub log_tx: Option<std::sync::mpsc::Sender<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub file_count: u32,
    pub total_file_count: u32,
    pub chunk_count: u32,
    pub symbolic_chunk_count: u32,
    pub vector_count: u32,
    pub embed_model: Option<String>,
}

/// A single indexable unit — either a tree-sitter symbol or a 50-line window.
#[derive(Debug, Clone)]
pub struct SemanticChunk {
    /// Stable embedding key: `sym.id` for symbol chunks, `"rel_path:idx"` for line chunks
    pub id: String,
    pub file_path: String,
    pub line_start: u64,
    #[allow(dead_code)]
    pub line_end: u64,
    pub content: String,
    #[allow(dead_code)]
    pub is_symbolic: bool,
}

/// Produce symbol-based chunks from a set of parsed symbols.
pub fn chunk_file_by_symbols(
    symbols: &[super::symbols::Symbol],
    rel_path: &str,
) -> Vec<SemanticChunk> {
    symbols
        .iter()
        .map(|sym| SemanticChunk {
            id: sym.id.clone(),
            file_path: rel_path.to_string(),
            line_start: sym.line_start,
            line_end: sym.line_end,
            content: sym.source.clone(),
            is_symbolic: true,
        })
        .collect()
}

/// Produce 50-line window chunks (fallback for files without tree-sitter support).
pub fn chunk_file_by_lines(content: &str, rel_path: &str) -> Vec<SemanticChunk> {
    let lines: Vec<&str> = content.lines().collect();
    lines
        .chunks(50)
        .enumerate()
        .map(|(idx, chunk)| {
            let line_start = idx as u64 * 50;
            SemanticChunk {
                id: format!("{rel_path}:{idx}"),
                file_path: rel_path.to_string(),
                line_start,
                line_end: line_start + chunk.len() as u64 - 1,
                content: chunk.join("\n"),
                is_symbolic: false,
            }
        })
        .collect()
}

pub fn build_schema() -> (Schema, Field, Field, Field, Field, Field) {
    let mut builder = Schema::builder();
    let file_path = builder.add_text_field("file_path", STRING | STORED);
    let content = builder.add_text_field("content", TEXT | STORED);
    let kind = builder.add_text_field("kind", STRING | STORED); // "bm25" | "symbol"
    let line_start = builder.add_u64_field("line_start", STORED);
    let symbol_id = builder.add_text_field("symbol_id", STRING | STORED);
    (
        builder.build(),
        file_path,
        content,
        kind,
        line_start,
        symbol_id,
    )
}

pub fn open_or_create_index(index_dir: &Path, reset: bool) -> tantivy::Result<Index> {
    if reset && index_dir.exists() {
        std::fs::remove_dir_all(index_dir)?;
    }
    std::fs::create_dir_all(index_dir)?;
    let (schema, _, _, _, _, _) = build_schema();
    if index_dir.join("meta.json").exists() && !reset {
        Index::open_in_dir(index_dir)
    } else {
        Index::create_in_dir(index_dir, schema)
    }
}

/// Index all text files in `path` using BM25 (tantivy).
/// If `opts.embed_provider` is configured, compute and store chunk embeddings
/// into `{index_dir}/embeddings.json`.
fn load_timestamps(dir: &Path) -> HashMap<String, u64> {
    let p = dir.join("file_timestamps.json");
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_timestamps(dir: &Path, ts: &HashMap<String, u64>) {
    if let Ok(s) = serde_json::to_string(ts) {
        let _ = std::fs::write(dir.join("file_timestamps.json"), s);
    }
}

fn file_mtime(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn index_directory(opts: IndexOptions) -> tantivy::Result<IndexStats> {
    // If the embedding model changed, invalidate the existing HNSW index by
    // treating it as a full reset for the embeddings (but keep BM25 incremental).
    let current_model_id = match &opts.embed_provider {
        crate::config::settings::EmbedProvider::Candle { model } => model.clone(),
        crate::config::settings::EmbedProvider::None
        | crate::config::settings::EmbedProvider::Legacy => String::new(),
    };
    let model_changed = HnswMeta::load(&opts.index_dir)
        .map(|m| m.model_id != current_model_id && !current_model_id.is_empty())
        .unwrap_or(false);
    macro_rules! log {
        ($($arg:tt)*) => {{
            let msg = format!($($arg)*);
            match &opts.log_tx {
                Some(tx) => { let _ = tx.send(msg); }
                None => eprintln!("{msg}"),
            }
        }};
    }

    if model_changed {
        log!("ecotokens: embedding model changed — rebuilding semantic index");
        let _ = std::fs::remove_file(opts.index_dir.join("hnsw_index.bin"));
        let _ = std::fs::remove_file(opts.index_dir.join("hnsw_meta.json"));
        let _ = std::fs::remove_file(opts.index_dir.join("embeddings.json"));
    }

    // T054 — migrate legacy embeddings.json → hnsw_index.bin on first run after upgrade
    let hnsw_bin = opts.index_dir.join("hnsw_index.bin");
    let legacy_json = opts.index_dir.join("embeddings.json");
    if !hnsw_bin.exists() && legacy_json.exists() {
        let legacy = super::embed::load_embeddings(&opts.index_dir);
        if !legacy.is_empty() {
            log!(
                "ecotokens: migrating {} legacy embeddings to hnsw_index.bin…",
                legacy.len()
            );
            let hnsw_data: Vec<(String, Vec<f32>)> = legacy.into_iter().collect();
            let hnsw = HnswIndex::build(&hnsw_data);
            let _ = hnsw.save(&opts.index_dir);
        }
    }

    let index = open_or_create_index(&opts.index_dir, opts.reset)?;
    let (_, file_path_field, content_field, kind_field, line_start_field, _symbol_id_field) =
        build_schema();

    let mut writer: IndexWriter = index.writer(200_000_000)?;

    // Incremental mode: load per-file timestamps to skip unchanged files
    let mut timestamps = if opts.reset {
        HashMap::new()
    } else {
        load_timestamps(&opts.index_dir)
    };
    let mut seen_paths: HashSet<String> = HashSet::new();

    let mut file_count = 0u32;
    let mut chunk_count = 0u32;
    let mut symbolic_chunk_count = 0u32;
    // Seed with existing embeddings so unchanged chunks keep their vectors.
    // Prefer hnsw_index.bin (current format); fall back to legacy embeddings.json.
    let mut embeddings: HashMap<String, Vec<f32>> = if opts.reset {
        HashMap::new()
    } else {
        HnswIndex::load(&opts.index_dir)
            .map(|h| h.to_embeddings())
            .unwrap_or_else(|| super::embed::load_embeddings(&opts.index_dir))
    };

    let walker = ignore::WalkBuilder::new(&opts.path)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !is_indexable_extension(ext) {
            continue;
        }

        if let Some(p) = &opts.progress {
            p.fetch_add(1, Ordering::Relaxed);
        }

        // Skip files larger than 50 MB to avoid memory exhaustion
        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if file_size > 50 * 1024 * 1024 {
            log!(
                "Skipping large file: {} ({} MB)",
                path.display(),
                file_size / 1024 / 1024
            );
            continue;
        }

        let rel_path = path
            .strip_prefix(&opts.path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());

        seen_paths.insert(rel_path.clone());

        // Skip unchanged files (incremental mode)
        let mtime = file_mtime(path);
        if !opts.reset && timestamps.get(&rel_path) == Some(&mtime) {
            continue;
        }

        // Remove stale docs for this file before re-adding
        let term = Term::from_field_text(file_path_field, &rel_path);
        writer.delete_term(term);

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // ── Symbolic extraction (tree-sitter for code, regex for docs) ──────
        let ext_str = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut raw_symbols = match ext_str {
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "cc" | "cxx"
            | "hpp" | "hh" | "hxx" => parse_symbols(path).unwrap_or_default(),
            "md" | "markdown" | "toml" | "json" | "yaml" | "yml" => {
                index_text_doc(path, &rel_path).unwrap_or_default()
            }
            _ => vec![],
        };
        // Fix IDs: parse_symbols uses just the basename; we need the rel_path prefix
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        for sym in &mut raw_symbols {
            if sym.file_path == filename {
                if let Some(suffix) = sym.id.strip_prefix(&format!("{filename}::")) {
                    sym.id = format!("{rel_path}::{suffix}");
                }
                sym.file_path = rel_path.clone();
            }
        }

        // ── Decide chunking strategy ─────────────────────────────────────────
        let bm25_chunks: Vec<SemanticChunk> = if !raw_symbols.is_empty() {
            // Symbol-aware chunks: each symbol = one chunk
            chunk_file_by_symbols(&raw_symbols, &rel_path)
        } else {
            // Fallback: 50-line windows
            chunk_file_by_lines(&content, &rel_path)
        };

        // ── BM25 indexing ───────────────────────────────────────────────────
        let is_symbolic_batch = !raw_symbols.is_empty();
        for chunk in &bm25_chunks {
            writer.add_document(doc!(
                file_path_field  => chunk.file_path.clone(),
                content_field    => chunk.content.clone(),
                kind_field       => "bm25",
                line_start_field => chunk.line_start,
                _symbol_id_field => chunk.id.clone(),
            ))?;
            chunk_count += 1;
            if is_symbolic_batch {
                symbolic_chunk_count += 1;
            }

            // Embedding (best-effort; skips if provider unavailable)
            if !embeddings.contains_key(&chunk.id) {
                if let Some(vec) = embed_text(&chunk.content, &opts.embed_provider) {
                    embeddings.insert(chunk.id.clone(), vec);
                }
            }
        }

        // ── Symbol-level tantivy index (for outline / lookup_symbol) ────────
        if !raw_symbols.is_empty() {
            let _ = write_symbols(&raw_symbols, &mut writer); // best-effort
        }

        timestamps.insert(rel_path, mtime);
        file_count += 1;
    }

    // Remove docs for files that no longer exist
    let deleted: Vec<String> = timestamps
        .keys()
        .filter(|p| !seen_paths.contains(*p))
        .cloned()
        .collect();
    for p in &deleted {
        let term = Term::from_field_text(file_path_field, p.as_str());
        writer.delete_term(term);
    }
    for p in deleted {
        timestamps.remove(&p);
    }

    writer.commit()?;

    save_timestamps(&opts.index_dir, &timestamps);

    // Build HNSW index when embeddings are available.
    let (final_vector_count, final_embed_model) = if !embeddings.is_empty() {
        let hnsw_data: Vec<(String, Vec<f32>)> = embeddings.into_iter().collect();
        let vector_count = hnsw_data.len();
        let hnsw = HnswIndex::build(&hnsw_data);
        if let Err(e) = hnsw.save(&opts.index_dir) {
            log!("ecotokens: warning: could not save HNSW index: {e}");
        }
        let model_id = match &opts.embed_provider {
            crate::config::settings::EmbedProvider::Candle { model } => model.clone(),
            crate::config::settings::EmbedProvider::None
            | crate::config::settings::EmbedProvider::Legacy => String::new(),
        };
        let dim = hnsw_data.iter().map(|(_, v)| v.len()).next().unwrap_or(0);
        let meta = HnswMeta {
            model_id: model_id.clone(),
            dimension: dim,
            vector_count,
            indexed_at: chrono::Utc::now().to_rfc3339(),
        };
        let _ = meta.save(&opts.index_dir);
        (vector_count as u32, Some(model_id))
    } else {
        (0, None)
    };

    Ok(IndexStats {
        file_count,
        total_file_count: timestamps.len() as u32,
        chunk_count,
        symbolic_chunk_count,
        vector_count: final_vector_count,
        embed_model: final_embed_model,
    })
}

/// Count files that would be considered indexable during indexing.
pub fn count_indexable_files(path: &Path) -> u64 {
    let walker = ignore::WalkBuilder::new(path)
        .hidden(false)
        .git_ignore(true)
        .build();

    walker
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            let ext = e.path().extension().and_then(|x| x.to_str()).unwrap_or("");
            is_indexable_extension(ext)
        })
        .count() as u64
}
