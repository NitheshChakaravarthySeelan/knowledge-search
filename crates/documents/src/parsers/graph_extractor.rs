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
    ///
    /// For supported programming languages, uses Tree-sitter for proper CST parsing.
    /// Falls back to regex heuristics for markdown/text and unsupported languages.
    pub fn extract(file_path: &str, content: &str, extension: Option<&str>) -> ExtractedGraphData {
        let ext = extension.unwrap_or("").to_lowercase();

        // Try Tree-sitter first for code languages
        if let Some(result) = crate::parsers::tree_sitter_ext::TreeSitterExtractor::extract(file_path, &ext, content) {
            return result;
        }

        // Fall back to regex-based extraction for non-code content
        match ext.as_str() {
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
        // The regex-based markdown extractor extracts file stems from link URLs,
        // not display text. For [[Page A]] it captures "Page A", but for
        // [Page C](page_c.md) it captures the file stem "page_c" from the URL.
        assert_eq!(result.edges.len(), 3);
        assert_eq!(result.edges[0].source_node_name, "doc");
        assert_eq!(result.edges[0].target_node_name, "Page A");
        assert_eq!(result.edges[1].target_node_name, "Page B");
        assert_eq!(result.edges[2].target_node_name, "page_c");
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
