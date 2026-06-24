use tree_sitter::{Parser, Query, QueryCursor, Language, StreamingIterator};
use super::graph_extractor::{ExtractedNode, ExtractedEdge, ExtractedGraphData};

/// Replaces the regex-based code extraction in `GraphExtractor` with proper
/// CST-based queries using Tree-sitter language grammars.
///
/// Why Tree-sitter instead of regex:
///   - Regex can't handle nested structures, generics, lifetimes, macros
///   - Tree-sitter produces a proper CST with named nodes even for malformed input
///   - Queries are declarative patterns over named node types
///   - Industry standard: GitHub Copilot, Sourcegraph, Zed, NeoVim all use it
pub struct TreeSitterExtractor;

impl TreeSitterExtractor {
    /// Dispatch to the appropriate language extractor based on file extension.
    pub fn extract(file_path: &str, ext: &str, content: &str) -> Option<ExtractedGraphData> {
        let source_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path)
            .to_string();

        match ext {
            "rs" | "rust" => Some(Self::extract_rust(&source_name, content)),
            "py" | "python" => Some(Self::extract_python(&source_name, content)),
            "js" | "javascript" => Some(Self::extract_javascript(&source_name, content)),
            "ts" | "typescript" => Some(Self::extract_typescript(&source_name, content)),
            "tsx" => Some(Self::extract_tsx(&source_name, content)),
            _ => None,
        }
    }

    fn extract_rust(source_name: &str, content: &str) -> ExtractedGraphData {
        Self::extract_with_language(source_name, content, tree_sitter_rust::LANGUAGE.into(), &[
            ("(struct_item name: (type_identifier) @name) @node",       "Class",    "DEFINES"),
            ("(enum_item name: (type_identifier) @name) @node",         "Class",    "DEFINES"),
            ("(trait_item name: (type_identifier) @name) @node",        "Class",    "DEFINES"),
            ("(union_item name: (type_identifier) @name) @node",        "Class",    "DEFINES"),
            ("(function_item name: (identifier) @name) @node",          "Function", "DEFINES"),
            ("(use_declaration) @node",                                  "",        "IMPORTS"),
            ("(impl_item trait: (type_identifier) @trait type: (type_identifier) @type) @node", "", "IMPLEMENTS"),
        ])
    }

    fn extract_python(source_name: &str, content: &str) -> ExtractedGraphData {
        Self::extract_with_language(source_name, content, tree_sitter_python::LANGUAGE.into(), &[
            ("(class_definition name: (identifier) @name) @node",       "Class",    "DEFINES"),
            ("(function_definition name: (identifier) @name) @node",    "Function", "DEFINES"),
            ("(import_statement) @node",                                 "",        "IMPORTS"),
            ("(import_from_statement) @node",                            "",        "IMPORTS"),
        ])
    }

    fn extract_javascript(source_name: &str, content: &str) -> ExtractedGraphData {
        Self::extract_with_language(source_name, content, tree_sitter_javascript::LANGUAGE.into(), &[
            ("(class_declaration name: (identifier) @name) @node",                                  "Class",    "DEFINES"),
            ("(function_declaration name: (identifier) @name) @node",                               "Function", "DEFINES"),
            ("(export_statement declaration: (function_declaration name: (identifier) @name)) @node", "Function", "DEFINES"),
            ("(variable_declarator name: (identifier) @name value: (arrow_function)) @node",        "Function", "DEFINES"),
            ("(import_statement) @node",                                                             "",        "IMPORTS"),
        ])
    }

    fn extract_typescript(source_name: &str, content: &str) -> ExtractedGraphData {
        Self::extract_with_language(source_name, content, tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), &[
            ("(class_declaration name: (identifier) @name) @node",                                  "Class",    "DEFINES"),
            ("(function_declaration name: (identifier) @name) @node",                               "Function", "DEFINES"),
            ("(export_statement declaration: (function_declaration name: (identifier) @name)) @node", "Function", "DEFINES"),
            ("(variable_declarator name: (identifier) @name value: (arrow_function)) @node",        "Function", "DEFINES"),
            ("(import_statement) @node",                                                             "",        "IMPORTS"),
            ("(interface_declaration name: (type_identifier) @name) @node",                         "Class",    "DEFINES"),
            ("(type_alias_declaration name: (type_identifier) @name) @node",                        "Class",    "DEFINES"),
        ])
    }

    fn extract_tsx(source_name: &str, content: &str) -> ExtractedGraphData {
        Self::extract_with_language(source_name, content, tree_sitter_typescript::LANGUAGE_TSX.into(), &[
            ("(class_declaration name: (identifier) @name) @node",                                  "Class",    "DEFINES"),
            ("(function_declaration name: (identifier) @name) @node",                               "Function", "DEFINES"),
            ("(export_statement declaration: (function_declaration name: (identifier) @name)) @node", "Function", "DEFINES"),
            ("(variable_declarator name: (identifier) @name value: (arrow_function)) @node",        "Function", "DEFINES"),
            ("(import_statement) @node",                                                             "",        "IMPORTS"),
            ("(interface_declaration name: (type_identifier) @name) @node",                         "Class",    "DEFINES"),
            ("(type_alias_declaration name: (type_identifier) @name) @node",                        "Class",    "DEFINES"),
        ])
    }

    /// Core extraction logic: parse content with the given language grammar,
    /// then run each query pattern to extract nodes and edges.
    fn extract_with_language(
        source_name: &str,
        content: &str,
        language: Language,
        queries: &[(&str, &str, &str)],
    ) -> ExtractedGraphData {
        let mut children: Vec<ExtractedNode> = Vec::new();
        let mut edges: Vec<ExtractedEdge> = Vec::new();

        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return ExtractedGraphData { children: vec![], edges: vec![] };
        }

        let tree = match parser.parse(content, None) {
            Some(t) => t,
            None => return ExtractedGraphData { children: vec![], edges: vec![] },
        };

        let root = tree.root_node();

        for &(pattern, node_type, relation) in queries {
            let query = match Query::new(&language, pattern) {
                Ok(q) => q,
                Err(_) => continue,
            };

            let capture_names: Vec<String> = (0..query.capture_names().len())
                .map(|i| query.capture_names()[i].to_string())
                .collect();

            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(&query, root, content.as_bytes());

            while let Some(match_) = matches.next() {
                let mut name_capture: Option<String> = None;
                let mut trait_capture: Option<String> = None;
                let mut type_capture: Option<String> = None;
                let mut node_start: Option<usize> = None;
                let mut node_end: Option<usize> = None;

                for capture in match_.captures.iter() {
                    let cap_name = &capture_names[capture.index as usize];
                    let node = capture.node;

                    match cap_name.as_str() {
                        "name" | "trait" | "type" => {
                            if let Ok(text) = node.utf8_text(content.as_bytes()) {
                                match cap_name.as_str() {
                                    "name"  => name_capture = Some(text.to_string()),
                                    "trait" => trait_capture = Some(text.to_string()),
                                    "type"  => type_capture = Some(text.to_string()),
                                    _ => {}
                                }
                            }
                        }
                        "node" | "impl" => {
                            node_start = Some(node.start_byte());
                            node_end = Some(node.end_byte());
                        }
                        _ => {}
                    }
                }

                match relation {
                    "DEFINES" => {
                        if let (Some(name), Some(start), Some(end)) = (&name_capture, node_start, node_end) {
                            children.push(ExtractedNode {
                                name: name.clone(),
                                node_type: node_type.to_string(),
                                content: content[start..end].to_string(),
                                start_offset: start,
                                end_offset: end,
                            });
                            edges.push(ExtractedEdge {
                                source_node_name: source_name.to_string(),
                                target_node_name: name.clone(),
                                relation_type: "DEFINES".to_string(),
                            });
                        }
                    }
                    "IMPORTS" => {
                        // Extract import target from the declaration text
                        // by removing language-specific keywords and semicolons.
                        if let (Some(start), Some(end)) = (node_start, node_end) {
                            let text = &content[start..end];
                            let target = Self::extract_import_target(text, source_name);
                            if let Some(t) = target {
                                edges.push(ExtractedEdge {
                                    source_node_name: source_name.to_string(),
                                    target_node_name: t,
                                    relation_type: "IMPORTS".to_string(),
                                });
                            }
                        }
                    }
                    "IMPLEMENTS" => {
                        if let (Some(trait_name), Some(type_name)) = (&trait_capture, &type_capture) {
                            edges.push(ExtractedEdge {
                                source_node_name: type_name.clone(),
                                target_node_name: trait_name.clone(),
                                relation_type: "IMPLEMENTS".to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        ExtractedGraphData { children, edges }
    }

    /// Given the full text of an import/use statement, extract the target path.
    /// Strips language-specific keywords (`use`, `import`, `from`, `as`, visibility modifiers, semicolons).
    fn extract_import_target(text: &str, _source_name: &str) -> Option<String> {
        let text = text.trim();

        // Rust: `use std::sync::Arc;` or `pub(crate) use std::sync::Arc;`
        if let Some(rest) = text.strip_suffix(';') {
            let rest = rest.trim();
            // Strip visibility modifier like `pub(crate) `
            let after_pub = rest
                .strip_prefix("pub")
                .or_else(|| Some(rest)) // no prefix, use as-is
                .map(|s| {
                    // If there's a parenthesized visibility, skip past it
                    let s = s.trim_start();
                    if s.starts_with('(') {
                        if let Some(close) = s.find(')') {
                            s[close + 1..].trim()
                        } else {
                            s
                        }
                    } else {
                        s
                    }
                })
                .unwrap_or(rest);

            let after_use = after_pub
                .strip_prefix("use")
                .map(|s| s.trim())
                .unwrap_or(after_pub);

            // Strip `as ...` alias
            let path = after_use.split(" as ").next().unwrap_or(after_use).trim();

            if !path.is_empty() {
                return Some(path.to_string());
            }
        }

        // Python: `import os` or `from os.path import join`
        if let Some(rest) = text.strip_prefix("from ") {
            // `from X import Y` → capture X
            if let Some(module) = rest.split(" import ").next() {
                if !module.is_empty() {
                    return Some(module.trim().to_string());
                }
            }
        }
        if let Some(rest) = text.strip_prefix("import ") {
            // `import X` or `import X as Y` → capture X
            let path = rest.split(" as ").next().unwrap_or(rest).trim();
            if !path.is_empty() {
                return Some(path.trim_end_matches(';').trim().to_string());
            }
        }

        // JavaScript/TypeScript: `import ... from "module"`
        if let Some(from_pos) = text.find(" from ") {
            let after_from = &text[from_pos + 6..];
            let module = after_from
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches(';')
                .trim();
            if !module.is_empty() {
                return Some(module.to_string());
            }
        }

        None
    }
}
