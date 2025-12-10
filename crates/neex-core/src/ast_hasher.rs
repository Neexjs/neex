//! AST Hasher - Content-based hashing that ignores comments/whitespace
//!
//! This is the KILLER FEATURE that makes neex better than Turbo/Nx
//!
//! Problem with Turbo/Nx:
//! - Add a comment → full rebuild
//! - Change whitespace → full rebuild
//!
//! Solution:
//! - Parse file with tree-sitter to AST
//! - Walk AST nodes, skip comment nodes
//! - Hash the AST structure (ignores comments/whitespace)
//! - Comment change = same hash = NO rebuild!

use anyhow::Result;
use blake3::Hasher as Blake3Hasher;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Hash a file's AST (ignores comments and whitespace)
/// Returns the same hash if only comments/whitespace changed
pub fn hash_ast(file_path: &Path, content: &str) -> Result<String> {
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Get appropriate language
    let language = match ext {
        "ts" | "mts" | "cts" => tree_sitter_typescript::language_typescript(),
        "tsx" => tree_sitter_typescript::language_tsx(),
        "js" | "mjs" | "cjs" | "jsx" => tree_sitter_javascript::language(),
        _ => {
            // Non-parseable file, hash raw content
            return hash_raw(content);
        }
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| anyhow::anyhow!("Language error: {}", e))?;

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return hash_raw(content),
    };

    // Walk AST and hash non-comment nodes
    let mut hasher = Blake3Hasher::new();
    hash_node(&tree.root_node(), content.as_bytes(), &mut hasher);

    Ok(hasher.finalize().to_hex().to_string())
}

/// Recursively hash AST nodes, skipping comments
fn hash_node(node: &Node, source: &[u8], hasher: &mut Blake3Hasher) {
    let kind = node.kind();

    // Skip comment nodes - THIS IS THE KEY!
    if kind.contains("comment") {
        return;
    }

    // Hash node kind (structure)
    hasher.update(kind.as_bytes());

    // For leaf nodes (no children), hash the actual text
    if node.child_count() == 0 {
        let text = &source[node.start_byte()..node.end_byte()];
        hasher.update(text);
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        hash_node(&child, source, hasher);
    }
}

/// Fallback: hash raw content (for non-JS/TS files)
pub fn hash_raw(content: &str) -> Result<String> {
    let mut hasher = Blake3Hasher::new();
    hasher.update(content.as_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

/// Check if file is a JS/TS file that can be AST-hashed
pub fn is_parseable(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    matches!(
        ext,
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "mts" | "cjs" | "cts"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_comment_does_not_change_hash() {
        let path = PathBuf::from("test.ts");

        // Code without comment
        let code1 = "const a = 1;";
        let hash1 = hash_ast(&path, code1).unwrap();

        // Same code with comment
        let code2 = "const a = 1; // this is a comment";
        let hash2 = hash_ast(&path, code2).unwrap();

        // Hashes should be the SAME!
        assert_eq!(hash1, hash2, "Comment should not change hash!");
    }

    #[test]
    fn test_code_change_changes_hash() {
        let path = PathBuf::from("test.ts");

        let code1 = "const a = 1;";
        let hash1 = hash_ast(&path, code1).unwrap();

        let code2 = "const a = 2;"; // Changed value
        let hash2 = hash_ast(&path, code2).unwrap();

        // Hashes should be DIFFERENT
        assert_ne!(hash1, hash2, "Code change should change hash!");
    }

    #[test]
    fn test_multiline_comment_ignored() {
        let path = PathBuf::from("test.js");

        let code1 = "function foo() { return 1; }";
        let hash1 = hash_ast(&path, code1).unwrap();

        let code2 = "/* Big comment */\nfunction foo() { return 1; }\n// Another comment";
        let hash2 = hash_ast(&path, code2).unwrap();

        // Hashes should be the SAME!
        assert_eq!(hash1, hash2, "Comments should not change hash!");
    }
}
