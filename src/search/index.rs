use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter, Term};

use super::embed::{embed_text, save_embeddings};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub file_count: u32,
    pub chunk_count: u32,
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
    let index = open_or_create_index(&opts.index_dir, opts.reset)?;
    let (_, file_path_field, content_field, kind_field, line_start_field, _symbol_id_field) =
        build_schema();

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    // Incremental mode: load per-file timestamps to skip unchanged files
    let mut timestamps = if opts.reset {
        HashMap::new()
    } else {
        load_timestamps(&opts.index_dir)
    };
    let mut seen_paths: HashSet<String> = HashSet::new();

    let mut file_count = 0u32;
    let mut chunk_count = 0u32;
    let mut embeddings: HashMap<String, Vec<f32>> = HashMap::new();

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
            eprintln!(
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

        // Split into chunks of 50 lines
        let lines: Vec<&str> = content.lines().collect();
        for (chunk_idx, chunk) in lines.chunks(50).enumerate() {
            let chunk_text = chunk.join("\n");
            let line_start = chunk_idx as u64 * 50;
            writer.add_document(doc!(
                file_path_field => rel_path.clone(),
                content_field => chunk_text.clone(),
                kind_field => "bm25",
                line_start_field => line_start,
            ))?;
            chunk_count += 1;

            // Optional embedding (best-effort; never blocks BM25 indexing).
            if let Some(vec) = embed_text(&chunk_text, &opts.embed_provider) {
                let key = format!("{}:{}", rel_path, chunk_idx);
                embeddings.insert(key, vec);
            }
        }
        // Symbolic indexing (tree-sitter for code, regex for docs)
        let symbol_ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut symbols = match symbol_ext {
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "c" | "h" | "cpp" | "cc" | "cxx"
            | "hpp" | "hh" | "hxx" => parse_symbols(path).unwrap_or_default(),
            "md" | "markdown" | "toml" | "json" | "yaml" | "yml" => {
                index_text_doc(path, &rel_path).unwrap_or_default()
            }
            _ => vec![],
        };
        // Fix up IDs and file_path: parse_symbols uses just the filename, but we
        // need the full project-relative path so that lookup_symbol IDs match.
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        for sym in &mut symbols {
            if sym.file_path == filename {
                if let Some(suffix) = sym.id.strip_prefix(&format!("{filename}::")) {
                    sym.id = format!("{rel_path}::{suffix}");
                }
                sym.file_path = rel_path.clone();
            }
        }
        if !symbols.is_empty() {
            let _ = write_symbols(&symbols, &mut writer); // best-effort
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

    // Save embeddings when available.
    if !embeddings.is_empty() {
        let _ = save_embeddings(&opts.index_dir, &embeddings);
    }

    Ok(IndexStats {
        file_count,
        chunk_count,
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
