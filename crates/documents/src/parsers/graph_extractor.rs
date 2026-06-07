use regex::Regex;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ExtractedNode {
    pub name: String,
    pub node_type: String, // "Section", "Class", "Method", "Function"
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone)]
pub struct ExtractedEdge {
    pub source_node_name: String,
    pub target_node_name: String,
    pub relation_type: String, // "REFERENCES", "CALLS", "IMPORTS", "DEFINES", "IMPLEMENTS"
}

#[derive(Debug, Clone)]
pub struct ExtractedGraphData {
    pub children: Vec<ExtractedNode>,
    pub edges: Vec<ExtractedEdge>,
}

pub struct GraphExtractor;

impl GraphExtractor {
    /// Extracts document/code nodes and relational edges from raw content.
    pub fn extract(file_path: &str, content: &str, extension: Option<&str>) -> ExtractedGraphData {
        let ext = extension.unwrap_or("").to_lowercase();
        match ext.as_str() {
            "rs" | "rust" => Self::extract_rust(file_path, content),
            "py" | "python" => Self::extract_python(file_path, content),
            "js" | "ts" | "javascript" | "typescript" => Self::extract_js_ts(file_path, content),
            "md" | "markdown" | "txt" | "" => Self::extract_markdown_or_text(file_path, content),
            _ => Self::extract_generic(file_path, content),
        }
    }

    /// Extract links and references from Markdown/Text (Wikilinks `[[Link]]` and standard links)
    fn extract_markdown_or_text(file_path: &str, content: &str) -> ExtractedGraphData {
        let mut edges = Vec::new();
        let file_name = std::path::Path::new(file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path)
            .to_string();

        // 1. Regex for wikilinks: [[Target Note]] or [[Target Note|Label]]
        let wikilink_re = Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap();
        let mut seen_edges = HashSet::new();

        for cap in wikilink_re.captures_iter(content) {
            if let Some(target) = cap.get(1) {
                let target_name = target.as_str().trim().to_string();
                if !target_name.is_empty() && target_name != file_name {
                    let key = (file_name.clone(), target_name.clone(), "REFERENCES".to_string());
                    if seen_edges.insert(key) {
                        edges.push(ExtractedEdge {
                            source_node_name: file_name.clone(),
                            target_node_name: target_name,
                            relation_type: "REFERENCES".to_string(),
                        });
                    }
                }
            }
        }

        // 2. Regex for standard markdown links to local files: [Label](target.md)
        let markdown_link_re = Regex::new(r"\[[^\]]*\]\(([^)]+)\)").unwrap();
        for cap in markdown_link_re.captures_iter(content) {
            if let Some(target) = cap.get(1) {
                let target_path = target.as_str().trim();
                // Check if it looks like a local file link (not HTTP)
                if !target_path.starts_with("http://") && !target_path.starts_with("https://") {
                    let target_name = std::path::Path::new(target_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(target_path)
                        .to_string();

                    if !target_name.is_empty() && target_name != file_name {
                        let key = (file_name.clone(), target_name.clone(), "REFERENCES".to_string());
                        if seen_edges.insert(key) {
                            edges.push(ExtractedEdge {
                                source_node_name: file_name.clone(),
                                target_node_name: target_name,
                                relation_type: "REFERENCES".to_string(),
                            });
                        }
                    }
                }
            }
        }

        ExtractedGraphData {
            children: vec![], // Markdown document nodes are handled by standard hierarchical chunker
            edges,
        }
    }

    /// Extract structure and dependencies from Rust code
    fn extract_rust(file_path: &str, content: &str) -> ExtractedGraphData {
        let mut children = Vec::new();
        let mut edges = Vec::new();
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path)
            .to_string();

        // Regex definitions for Rust code constructs
        let struct_re = Regex::new(r"(?m)^(?:pub\s+)?(?:struct|enum|trait)\s+([A-Za-z0-9_]+)").unwrap();
        let impl_re = Regex::new(r"(?m)^impl(?:\s+([A-Za-z0-9_]+)\s+for)?\s+([A-Za-z0-9_]+)").unwrap();
        let fn_re = Regex::new(r"(?m)^(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z0-9_]+)\s*\(").unwrap();
        let use_re = Regex::new(r"(?m)^use\s+([^;]+);").unwrap();

        // 1. Extract Imports (IMPORTS relationship)
        for cap in use_re.captures_iter(content) {
            if let Some(import_path) = cap.get(1) {
                let path_str = import_path.as_str().trim().to_string();
                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: path_str,
                    relation_type: "IMPORTS".to_string(),
                });
            }
        }

        // 2. Extract Structs/Enums/Traits (DEFINES relationship)
        for cap in struct_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let struct_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: struct_name.clone(),
                    node_type: "Class".to_string(), // Keep consistent mapping
                    content: format!("struct/enum/trait {}", struct_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: struct_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        // 3. Extract Functions/Methods (DEFINES relationship)
        for cap in fn_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let fn_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: fn_name.clone(),
                    node_type: "Function".to_string(),
                    content: format!("fn {}", fn_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: fn_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        // 4. Extract impl blocks (IMPLEMENTS relationship)
        for cap in impl_re.captures_iter(content) {
            if let Some(trait_name_match) = cap.get(1) {
                // impl Trait for Struct
                let trait_name = trait_name_match.as_str().to_string();
                if let Some(struct_name_match) = cap.get(2) {
                    let struct_name = struct_name_match.as_str().to_string();
                    edges.push(ExtractedEdge {
                        source_node_name: struct_name,
                        target_node_name: trait_name,
                        relation_type: "IMPLEMENTS".to_string(),
                    });
                }
            }
        }

        ExtractedGraphData { children, edges }
    }

    /// Extract structure and dependencies from Python code
    fn extract_python(file_path: &str, content: &str) -> ExtractedGraphData {
        let mut children = Vec::new();
        let mut edges = Vec::new();
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path)
            .to_string();

        let class_re = Regex::new(r"(?m)^class\s+([A-Za-z0-9_]+)").unwrap();
        let def_re = Regex::new(r"(?m)^def\s+([A-Za-z0-9_]+)\s*\(").unwrap();
        let import_re = Regex::new(r"(?m)^(?:import\s+([A-Za-z0-9_.]+)|from\s+([A-Za-z0-9_.]+)\s+import)").unwrap();

        // 1. Extract Imports
        for cap in import_re.captures_iter(content) {
            let import_target = cap.get(1).or_else(|| cap.get(2));
            if let Some(target) = import_target {
                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: target.as_str().to_string(),
                    relation_type: "IMPORTS".to_string(),
                });
            }
        }

        // 2. Extract Classes
        for cap in class_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let class_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: class_name.clone(),
                    node_type: "Class".to_string(),
                    content: format!("class {}", class_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: class_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        // 3. Extract Functions
        for cap in def_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let fn_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: fn_name.clone(),
                    node_type: "Function".to_string(),
                    content: format!("def {}", fn_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: fn_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        ExtractedGraphData { children, edges }
    }

    /// Extract structure and dependencies from JavaScript / TypeScript code
    fn extract_js_ts(file_path: &str, content: &str) -> ExtractedGraphData {
        let mut children = Vec::new();
        let mut edges = Vec::new();
        let file_name = std::path::Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path)
            .to_string();

        let class_re = Regex::new(r"(?m)^class\s+([A-Za-z0-9_]+)").unwrap();
        let function_re = Regex::new(r"(?m)^(?:export\s+)?(?:async\s+)?function\s+([A-Za-z0-9_]+)\s*\(").unwrap();
        let import_re = Regex::new(r#"(?m)import\s+(?:[^'"]+)\s+from\s+['"]([^'"]+)['"]"#).unwrap();

        // 1. Extract Imports
        for cap in import_re.captures_iter(content) {
            if let Some(target) = cap.get(1) {
                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: target.as_str().to_string(),
                    relation_type: "IMPORTS".to_string(),
                });
            }
        }

        // 2. Extract Classes
        for cap in class_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let class_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: class_name.clone(),
                    node_type: "Class".to_string(),
                    content: format!("class {}", class_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: class_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        // 3. Extract Functions
        for cap in function_re.captures_iter(content) {
            if let Some(name) = cap.get(1) {
                let fn_name = name.as_str().to_string();
                let match_range = cap.get(0).unwrap();
                children.push(ExtractedNode {
                    name: fn_name.clone(),
                    node_type: "Function".to_string(),
                    content: format!("function {}", fn_name),
                    start_offset: match_range.start(),
                    end_offset: match_range.end(),
                });

                edges.push(ExtractedEdge {
                    source_node_name: file_name.clone(),
                    target_node_name: fn_name,
                    relation_type: "DEFINES".to_string(),
                });
            }
        }

        ExtractedGraphData { children, edges }
    }

    /// Generic fallback extractor
    fn extract_generic(_file_path: &str, _content: &str) -> ExtractedGraphData {
        ExtractedGraphData {
            children: vec![],
            edges: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_markdown_wikilinks() {
        let content = "This is [[Page A]] referencing [[Page B|Label]].\nAlso a standard link to [Page C](page_c.md)";
        let result = GraphExtractor::extract("doc.md", content, Some("md"));
        assert_eq!(result.edges.len(), 3);
        assert_eq!(result.edges[0].source_node_name, "doc");
        assert_eq!(result.edges[0].target_node_name, "Page A");
        assert_eq!(result.edges[1].target_node_name, "Page B");
        assert_eq!(result.edges[2].target_node_name, "Page C");
    }

    #[test]
    fn test_extract_rust_syntax() {
        let content = "use std::sync::Arc;\nuse crate::utils::logger;\npub struct UserService;\nimpl Log for UserService {}\nfn login() {}";
        let result = GraphExtractor::extract("user.rs", content, Some("rs"));
        assert!(result.edges.iter().any(|e| e.relation_type == "IMPORTS" && e.target_node_name == "std::sync::Arc"));
        assert!(result.children.iter().any(|c| c.name == "UserService" && c.node_type == "Class"));
        assert!(result.children.iter().any(|c| c.name == "login" && c.node_type == "Function"));
    }
}
