use regex::Regex;
use std::collections::{HashMap, HashSet};
use crate::parsers::graph_extractor::{GraphExtractor, ExtractedGraphData};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedRelation {
    pub source: String,
    pub target: String,
    pub relation_type: String,
}

pub struct EntityExtractor;

impl EntityExtractor {
    /// Extracts entities and relations from document content.
    /// For code files, delegates to GraphExtractor.
    /// For text, uses regex heuristics to extract key terms.
    pub fn extract_for_content(
        file_path: &str,
        content: &str,
        file_ext: Option<&str>,
    ) -> (Vec<ExtractedEntity>, Vec<ExtractedRelation>) {
        let ext = file_ext.unwrap_or("").to_lowercase();

        match ext.as_str() {
            "rs" | "rust" | "py" | "python" | "js" | "ts" | "javascript" | "typescript"
            | "go" | "java" | "cpp" | "c" | "h" | "hpp" => {
                Self::extract_from_code(file_path, content, file_ext)
            }
            _ => Self::extract_from_text(file_path, content),
        }
    }

    /// Extracts entities from code using GraphExtractor, plus keyword heuristics.
    fn extract_from_code(
        file_path: &str,
        content: &str,
        ext: Option<&str>,
    ) -> (Vec<ExtractedEntity>, Vec<ExtractedRelation>) {
        let graph: ExtractedGraphData = GraphExtractor::extract(file_path, content, ext);

        let mut entities: Vec<ExtractedEntity> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for child in &graph.children {
            if seen.insert(child.name.clone()) {
                let entity_type = match child.node_type.as_str() {
                    "Class" => "class",
                    "Function" | "Method" => "function",
                    "Section" => "section",
                    _ => "code_element",
                };
                entities.push(ExtractedEntity {
                    name: child.name.clone(),
                    entity_type: entity_type.to_string(),
                    confidence: 0.9,
                });
            }
        }

        // Also extract import targets as entities
        for edge in &graph.edges {
            if edge.relation_type == "IMPORTS" {
                let target = edge.target_node_name.split("::").last().unwrap_or(&edge.target_node_name).to_string();
                if seen.insert(target.clone()) {
                    entities.push(ExtractedEntity {
                        name: target,
                        entity_type: "dependency".to_string(),
                        confidence: 0.8,
                    });
                }
            }
        }

        let relations: Vec<ExtractedRelation> = graph.edges.into_iter()
            .filter(|e| e.relation_type != "IMPORTS")
            .map(|e| ExtractedRelation {
                source: e.source_node_name,
                target: e.target_node_name,
                relation_type: e.relation_type,
            })
            .collect();

        (entities, relations)
    }

    /// Extracts entities from natural language text using heuristics.
    fn extract_from_text(
        _file_path: &str,
        content: &str,
    ) -> (Vec<ExtractedEntity>, Vec<ExtractedRelation>) {
        let mut entities: HashMap<String, (String, f32)> = HashMap::new();
        let mut relations: Vec<ExtractedRelation> = Vec::new();

        // Heuristic 1: Multi-word capitalized phrases (proper nouns / concepts)
        let phrase_re = Regex::new(r#"(?:^|[.!?]\s+)([A-Z][a-z]+(?:\s+[A-Z][a-z]+)+)"#).unwrap();
        for cap in phrase_re.captures_iter(content) {
            let phrase = cap.get(1).unwrap().as_str().trim().to_string();
            if phrase.len() > 3 {
                let entry = entities.entry(phrase)
                    .or_insert_with(|| ("concept".to_string(), 0.0));
                entry.1 += 0.1;
            }
        }

        // Heuristic 2: Technical terms (CamelCase, snake_case, UPPER_CASE)
        let tech_re = Regex::new(r"\b([A-Z][a-z0-9]+(?:[A-Z][a-z0-9]+)+)\b").unwrap();
        for cap in tech_re.captures_iter(content) {
            let term = cap.get(1).unwrap().as_str().to_string();
            let entry = entities.entry(term)
                .or_insert_with(|| ("technology".to_string(), 0.0));
            entry.1 += 0.15;
        }

        // Heuristic 3: Quoted terms ("important concept")
        let quote_re = Regex::new(r#""([^"]{3,})""#).unwrap();
        for cap in quote_re.captures_iter(content) {
            let term = cap.get(1).unwrap().as_str().to_string();
            let entry = entities.entry(term)
                .or_insert_with(|| ("term".to_string(), 0.0));
            entry.1 += 0.2;
        }

        // Heuristic 4: Acronyms (e.g., RAG, LLM, API, RRF, RoPE)
        let acronym_re = Regex::new(r"\b([A-Z]{2,5})\b").unwrap();
        for cap in acronym_re.captures_iter(content) {
            let term = cap.get(1).unwrap().as_str().to_string();
            if term.len() >= 2 {
                let entry = entities.entry(term)
                    .or_insert_with(|| ("acronym".to_string(), 0.0));
                entry.1 += 0.12;
            }
        }

        // Heuristic 5: Co-occurrence within 100 chars => relation
        let names: Vec<String> = entities.keys().cloned().collect();
        let content_lower = content.to_lowercase();
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                let a = &names[i];
                let b = &names[j];
                if let (Some(pos_a), Some(pos_b)) = (
                    content_lower.find(&a.to_lowercase()),
                    content_lower.find(&b.to_lowercase()),
                ) {
                    let dist = (pos_a as isize - pos_b as isize).unsigned_abs();
                    if dist < 100 {
                        relations.push(ExtractedRelation {
                            source: a.clone(),
                            target: b.clone(),
                            relation_type: "CO_OCCURS".to_string(),
                        });
                    }
                }
            }
        }

        let extracted: Vec<ExtractedEntity> = entities.into_iter()
            .map(|(name, (entity_type, confidence))| ExtractedEntity {
                name,
                entity_type,
                confidence: confidence.min(1.0),
            })
            .collect();

        (extracted, relations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_entities() {
        let content = "The RoPE (Rotary Position Embedding) is a novel method. \
                       RAG systems use both LLMs and vector databases. \
                       The Rotary Position Embedding enables better attention.";
        let (entities, relations) = EntityExtractor::extract_for_content("doc.md", content, Some("md"));
        assert!(!entities.is_empty(), "should extract entities");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Rotary Position Embedding"));
        assert!(names.contains(&"RAG"));
        assert!(names.contains(&"LLMs"));
        // Should have co-occurrence relations
        assert!(!relations.is_empty(), "should have relations from co-occurrence");
    }

    #[test]
    fn test_extract_code_entities() {
        let content = "pub struct UserService;\nimpl Log for UserService {}\nfn login() {}";
        let (entities, _) = EntityExtractor::extract_for_content("user.rs", content, Some("rs"));
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"UserService"));
        assert!(names.contains(&"login"));
    }
}
