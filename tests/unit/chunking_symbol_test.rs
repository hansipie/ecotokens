use ecotokens::search::symbols::Symbol;
use std::io::Write;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    // T010 — Symbol doit avoir line_end
    #[test]
    fn symbol_has_line_end() {
        let sym = Symbol {
            id: "foo.rs::bar#fn".to_string(),
            name: "bar".to_string(),
            kind: "fn".to_string(),
            file_path: "foo.rs".to_string(),
            line_start: 10,
            line_end: 25,
            source: "fn bar() {}".to_string(),
        };
        assert_eq!(sym.line_end, 25);
        assert!(sym.line_end >= sym.line_start);
    }

    // T041 — Un chunk symbolique doit couvrir la fonction entière
    #[test]
    fn symbol_chunk_covers_full_function() {
        use ecotokens::search::symbols::parse_symbols;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("big_fn.rs");

        // Build a Rust function spanning 80 lines
        let mut src = String::from("fn large_function(x: i32) -> i32 {\n");
        for i in 0..78 {
            src.push_str(&format!("    let _v{i} = x + {i};\n"));
        }
        src.push_str("    x\n}\n");

        std::fs::write(&path, &src).unwrap();

        let symbols = parse_symbols(&path).expect("parse failed");
        assert!(!symbols.is_empty(), "expected at least one symbol");

        let fn_sym = symbols
            .iter()
            .find(|s| s.name == "large_function")
            .expect("large_function symbol not found");

        assert!(
            fn_sym.line_end - fn_sym.line_start >= 79,
            "expected chunk to span ≥79 lines, got {}-{}={}",
            fn_sym.line_end,
            fn_sym.line_start,
            fn_sym.line_end - fn_sym.line_start
        );
        assert!(
            fn_sym.source.contains("large_function"),
            "source should contain fn name"
        );
        assert!(
            fn_sym.source.contains("x\n}"),
            "source should contain end of function"
        );
    }

    // T042 — Les fichiers sans symboles tree-sitter tombent en fallback ligne
    #[test]
    fn no_symbols_falls_back_to_line_chunks() {
        use ecotokens::search::index::{chunk_file_by_lines, chunk_file_by_symbols};
        use ecotokens::search::symbols::parse_symbols;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("plain.txt");

        // 120 lines of plain text → no tree-sitter symbols
        let content: String = (0..120).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, &content).unwrap();

        let symbols = parse_symbols(&path).unwrap_or_default();
        assert!(symbols.is_empty(), "txt file should have no symbols");

        let chunks = chunk_file_by_lines(&content, "plain.txt");
        assert!(
            chunks.len() >= 2,
            "120 lines should produce ≥2 chunks of 50 lines"
        );
        for c in &chunks {
            assert!(c.content.len() > 0, "chunk content should not be empty");
        }
    }

    // T043 — Les clés d'embedding doivent être stables entre deux runs
    #[test]
    fn symbol_chunk_id_is_stable() {
        use ecotokens::search::symbols::parse_symbols;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("stable.rs");
        std::fs::write(&path, "fn hello() { println!(\"hello\"); }\n").unwrap();

        let run1: Vec<String> = parse_symbols(&path)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        let run2: Vec<String> = parse_symbols(&path)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();

        assert_eq!(run1, run2, "symbol IDs must be identical across runs");
    }
}
