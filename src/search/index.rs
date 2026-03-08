use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexWriter};

use super::embed::{embed_text, save_embeddings};
use super::symbols::{parse_symbols, write_symbols};
use super::text_docs::index_text_doc;

#[derive(Debug, Clone)]
pub struct IndexOptions {
    pub reset: bool,
    pub path: PathBuf,
    pub index_dir: PathBuf,
    pub progress: Option<Arc<AtomicUsize>>,
    /// Provider d'embeddings (None = BM25 seul)
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
    (builder.build(), file_path, content, kind, line_start, symbol_id)
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
/// Si `opts.embed_provider` est configuré, calcule et stocke les embeddings
/// de chaque chunk dans `{index_dir}/embeddings.json`.
pub fn index_directory(opts: IndexOptions) -> tantivy::Result<IndexStats> {
    let index = open_or_create_index(&opts.index_dir, opts.reset)?;
    let (_, file_path_field, content_field, kind_field, line_start_field, _symbol_id_field) = build_schema();

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    // Clear existing docs if incremental (simple strategy: delete all, re-add)
    if !opts.reset && opts.index_dir.join("meta.json").exists() {
        writer.delete_all_documents()?;
    }

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

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rel_path = path.strip_prefix(&opts.path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string_lossy().into_owned());

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

            // Embedding optionnel (best-effort, sans bloquer l'indexation BM25)
            if let Some(vec) = embed_text(&chunk_text, &opts.embed_provider) {
                let key = format!("{}:{}", rel_path, chunk_idx);
                embeddings.insert(key, vec);
            }
        }
        // Symbolic indexing (tree-sitter for code, regex for docs)
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mut symbols = match ext {
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" => {
                parse_symbols(path).unwrap_or_default()
            }
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

        file_count += 1;
        if let Some(p) = &opts.progress {
            p.fetch_add(1, Ordering::Relaxed);
        }
    }

    writer.commit()?;

    // Sauvegarder les embeddings s'il y en a
    if !embeddings.is_empty() {
        let _ = save_embeddings(&opts.index_dir, &embeddings);
    }

    Ok(IndexStats { file_count, chunk_count })
}

fn is_indexable_extension(ext: &str) -> bool {
    matches!(ext, "rs" | "py" | "js" | "ts" | "md" | "toml" | "json" | "yaml" | "yml" | "txt")
}
