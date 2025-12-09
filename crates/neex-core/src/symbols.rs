//! Symbol Extractor - Phase 8.1
//!
//! Extract exports/imports from JS/TS files at symbol level.
//! Uses tree-sitter (already in project from Phase 3).
//!
//! This enables 10x reduction in rebuilds by tracking changes
//! at function/class level instead of file level.

use anyhow::Result;
use blake3::Hasher as Blake3Hasher;
use std::path::Path;
use tree_sitter::Parser;

/// Symbol kind
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Const,
    Variable,
    Type,
    Interface,
    Enum,
}

/// Exported symbol with its hash
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub hash: String,
    pub line: usize,
}

/// Import statement
#[derive(Debug, Clone)]
pub struct Import {
    pub from: String,
    pub symbols: Vec<String>,
    pub line: usize,
}

/// All symbols in a file
#[derive(Debug, Clone, Default)]
pub struct FileSymbols {
    pub exports: Vec<Symbol>,
    pub imports: Vec<Import>,
}

/// Extract symbols from JS/TS source code
pub fn extract_symbols(source: &str, is_typescript: bool) -> Result<FileSymbols> {
    let mut parser = Parser::new();
    
    let language = if is_typescript {
        tree_sitter_typescript::language_tsx()
    } else {
        tree_sitter_javascript::language()
    };
    
    parser.set_language(&language)?;

    
    let tree = parser.parse(source, None)
        .ok_or_else(|| anyhow::anyhow!("Parse failed"))?;
    
    let root = tree.root_node();
    let bytes = source.as_bytes();
    
    let mut symbols = FileSymbols::default();
    
    // Extract exports
    extract_exports(&mut symbols, root, bytes)?;
    
    // Extract imports
    extract_imports(&mut symbols, root, bytes)?;
    
    Ok(symbols)
}

/// Extract exported symbols
fn extract_exports(symbols: &mut FileSymbols, root: tree_sitter::Node, source: &[u8]) -> Result<()> {
    let mut cursor = root.walk();
    
    for node in root.children(&mut cursor) {
        // export function name() { ... }
        // export const name = ...
        // export class Name { ... }
        if node.kind() == "export_statement" {
            if let Some(decl) = node.child_by_field_name("declaration") {
                extract_declaration(symbols, decl, source)?;
            }
            // Handle: export { a, b }
            if let Some(clause) = node.child_by_field_name("value") {
                if clause.kind() == "export_clause" {
                    extract_export_clause(symbols, clause, source)?;
                }
            }
        }
        
        // export default function() { ... }
        if node.kind() == "export_default_declaration" {
            if let Some(child) = node.child(1) {
                let name = "default".to_string();
                let kind = match child.kind() {
                    "function_declaration" | "function" => SymbolKind::Function,
                    "class_declaration" | "class" => SymbolKind::Class,
                    _ => SymbolKind::Variable,
                };
                let hash = hash_node(child, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind,
                    hash,
                    line: child.start_position().row + 1,
                });
            }
        }
    }
    
    Ok(())
}

/// Extract from: function/class/const declaration
fn extract_declaration(symbols: &mut FileSymbols, node: tree_sitter::Node, source: &[u8]) -> Result<()> {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let hash = hash_node(node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    hash,
                    line: node.start_position().row + 1,
                });
            }
        }
        
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let hash = hash_node(node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Class,
                    hash,
                    line: node.start_position().row + 1,
                });
            }
        }
        
        "lexical_declaration" | "variable_declaration" => {
            // const/let/var declarations
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declarator" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = node_text(name_node, source);
                        let hash = hash_node(child, source);
                        
                        symbols.exports.push(Symbol {
                            name,
                            kind: SymbolKind::Const,
                            hash,
                            line: child.start_position().row + 1,
                        });
                    }
                }
            }
        }
        
        "type_alias_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let hash = hash_node(node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Type,
                    hash,
                    line: node.start_position().row + 1,
                });
            }
        }
        
        "interface_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let hash = hash_node(node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Interface,
                    hash,
                    line: node.start_position().row + 1,
                });
            }
        }
        
        "enum_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = node_text(name_node, source);
                let hash = hash_node(node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Enum,
                    hash,
                    line: node.start_position().row + 1,
                });
            }
        }
        
        _ => {}
    }
    
    Ok(())
}

/// Extract from: export { a, b, c }
fn extract_export_clause(symbols: &mut FileSymbols, node: tree_sitter::Node, source: &[u8]) -> Result<()> {
    let mut cursor = node.walk();
    
    for child in node.children(&mut cursor) {
        if child.kind() == "export_specifier" {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(name_node, source);
                
                symbols.exports.push(Symbol {
                    name,
                    kind: SymbolKind::Variable, // Could be anything
                    hash: String::new(), // No body to hash
                    line: child.start_position().row + 1,
                });
            }
        }
    }
    
    Ok(())
}

/// Extract import statements
fn extract_imports(symbols: &mut FileSymbols, root: tree_sitter::Node, source: &[u8]) -> Result<()> {
    let mut cursor = root.walk();
    
    for node in root.children(&mut cursor) {
        if node.kind() == "import_statement" {
            let mut import = Import {
                from: String::new(),
                symbols: Vec::new(),
                line: node.start_position().row + 1,
            };
            
            // Get source: from "module"
            if let Some(source_node) = node.child_by_field_name("source") {
                let text = node_text(source_node, source);
                // Remove quotes
                import.from = text.trim_matches(|c| c == '"' || c == '\'').to_string();
            }
            
            // Get imported symbols
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                if child.kind() == "import_clause" {
                    extract_import_clause(&mut import, child, source);
                }
            }
            
            if !import.from.is_empty() {
                symbols.imports.push(import);
            }
        }
    }
    
    Ok(())
}

/// Extract symbols from import clause
fn extract_import_clause(import: &mut Import, node: tree_sitter::Node, source: &[u8]) {
    let mut cursor = node.walk();
    
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                // import foo from "..."
                import.symbols.push(node_text(child, source));
            }
            "named_imports" => {
                // import { a, b } from "..."
                let mut spec_cursor = child.walk();
                for spec in child.children(&mut spec_cursor) {
                    if spec.kind() == "import_specifier" {
                        if let Some(name) = spec.child_by_field_name("name") {
                            import.symbols.push(node_text(name, source));
                        }
                    }
                }
            }
            "namespace_import" => {
                // import * as foo from "..."
                if let Some(alias) = child.child(2) {
                    import.symbols.push(format!("* as {}", node_text(alias, source)));
                }
            }
            _ => {}
        }
    }
}

/// Get text of a node
fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    String::from_utf8_lossy(&source[node.start_byte()..node.end_byte()]).to_string()
}

/// Hash a node's content
fn hash_node(node: tree_sitter::Node, source: &[u8]) -> String {
    let content = &source[node.start_byte()..node.end_byte()];
    let mut hasher = Blake3Hasher::new();
    hasher.update(content);
    hasher.finalize().to_hex()[..16].to_string()
}

/// Extract from file path
pub fn extract_from_file(path: &Path) -> Result<FileSymbols> {
    let content = std::fs::read_to_string(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_ts = matches!(ext, "ts" | "tsx" | "mts" | "cts");
    extract_symbols(&content, is_ts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_function() {
        let code = r#"export function formatDate(d) { return d.toString(); }"#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.exports.len(), 1);
        assert_eq!(symbols.exports[0].name, "formatDate");
        assert_eq!(symbols.exports[0].kind, SymbolKind::Function);
        assert!(!symbols.exports[0].hash.is_empty());
    }

    #[test]
    fn test_export_const() {
        let code = r#"export const VERSION = "1.0.0";"#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.exports.len(), 1);
        assert_eq!(symbols.exports[0].name, "VERSION");
        assert_eq!(symbols.exports[0].kind, SymbolKind::Const);
    }

    #[test]
    fn test_export_class() {
        let code = r#"export class User { constructor() {} }"#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.exports.len(), 1);
        assert_eq!(symbols.exports[0].name, "User");
        assert_eq!(symbols.exports[0].kind, SymbolKind::Class);
    }

    #[test]
    fn test_multiple_exports() {
        let code = r#"
            export function foo() {}
            export const BAR = 1;
            export class Baz {}
        "#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.exports.len(), 3);
    }

    #[test]
    fn test_import_named() {
        let code = r#"import { formatDate, formatNumber } from "@my/utils";"#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.imports.len(), 1);
        assert_eq!(symbols.imports[0].from, "@my/utils");
        assert_eq!(symbols.imports[0].symbols, vec!["formatDate", "formatNumber"]);
    }

    #[test]
    fn test_import_default() {
        let code = r#"import React from "react";"#;
        let symbols = extract_symbols(code, false).unwrap();
        
        assert_eq!(symbols.imports.len(), 1);
        assert_eq!(symbols.imports[0].from, "react");
        assert_eq!(symbols.imports[0].symbols, vec!["React"]);
    }

    #[test]
    fn test_typescript_interface() {
        let code = r#"export interface User { name: string; }"#;
        let symbols = extract_symbols(code, true).unwrap();
        
        assert_eq!(symbols.exports.len(), 1);
        assert_eq!(symbols.exports[0].name, "User");
        assert_eq!(symbols.exports[0].kind, SymbolKind::Interface);
    }

    #[test]
    fn test_hash_changes_with_content() {
        let code1 = r#"export function foo() { return 1; }"#;
        let code2 = r#"export function foo() { return 2; }"#;
        
        let s1 = extract_symbols(code1, false).unwrap();
        let s2 = extract_symbols(code2, false).unwrap();
        
        assert_ne!(s1.exports[0].hash, s2.exports[0].hash);
    }
}
