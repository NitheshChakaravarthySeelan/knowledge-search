use std::collections::HashMap;
use std::sync::LazyLock;

/// Maps common technical acronyms to their full forms so the embedding model
/// can match against documents that spell out the term.
static ACRONYM_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("RAG", "Retrieval Augmented Generation");
    m.insert("LLM", "Large Language Model");
    m.insert("LLMS", "Large Language Models");
    m.insert("BM25", "BM25 ranking function");
    m.insert("RRF", "Reciprocal Rank Fusion");
    m.insert("HNSW", "Hierarchical Navigable Small World");
    m.insert("NER", "Named Entity Recognition");
    m.insert("NLP", "Natural Language Processing");
    m.insert("SIMD", "Single Instruction Multiple Data");
    m.insert("GIL", "Global Interpreter Lock");
    m.insert("API", "Application Programming Interface");
    m.insert("AST", "Abstract Syntax Tree");
    m.insert("CST", "Concrete Syntax Tree");
    m.insert("DSL", "Domain Specific Language");
    m.insert("ORM", "Object Relational Mapping");
    m.insert("UUID", "Universally Unique Identifier");
    m.insert("JWT", "JSON Web Token");
    m.insert("SHA", "Secure Hash Algorithm");
    m.insert("AES", "Advanced Encryption Standard");
    m.insert("TLS", "Transport Layer Security");
    m.insert("ACL", "Access Control List");
    m.insert("TTL", "Time To Live");
    m.insert("CLI", "Command Line Interface");
    m.insert("SDK", "Software Development Kit");
    m.insert("IDE", "Integrated Development Environment");
    m.insert("JSON", "JavaScript Object Notation");
    m.insert("YAML", "YAML Ain't Markup Language");
    m.insert("TOML", "Tom's Obvious Minimal Language");
    m.insert("HTML", "HyperText Markup Language");
    m.insert("XML", "eXtensible Markup Language");
    m.insert("CSS", "Cascading Style Sheets");
    m.insert("SQL", "Structured Query Language");
    m.insert("HTTP", "HyperText Transfer Protocol");
    m.insert("CSV", "Comma Separated Values");
    m.insert("PDF", "Portable Document Format");
    m.insert("DB", "Database");
    m.insert("CPU", "Central Processing Unit");
    m.insert("GPU", "Graphics Processing Unit");
    m.insert("RAM", "Random Access Memory");
    m.insert("RBAC", "Role Based Access Control");
    m
});

/// The result of transforming a raw user query into search-optimized forms.
pub struct TransformedQuery {
    /// Primary query with acronyms expanded and noise removed.
    pub primary: String,
    /// Alternative query variants for multi-pass or sub-query search.
    pub variants: Vec<String>,
}

/// Transforms a raw user query into search-optimized forms before retrieval.
///
/// Techniques:
/// - Acronym expansion: "RAG" → "Retrieval Augmented Generation"
/// - Compound decomposition: splits "? ... ?" into sub-queries
pub struct QueryTransformer;

impl QueryTransformer {
    /// Runs the full transformation pipeline on a user query.
    pub fn transform(query: &str) -> TransformedQuery {
        let expanded = Self::expand_acronyms(query);
        let variants = Self::decompose(query);
        TransformedQuery {
            primary: expanded,
            variants,
        }
    }

    /// Expands known acronyms to their full forms (whole-word match only).
    fn expand_acronyms(query: &str) -> String {
        let mut result = query.to_string();
        let mut keys: Vec<&&str> = ACRONYM_MAP.keys().collect();
        keys.sort_by(|a, b| b.len().cmp(&a.len()));
        for acronym in keys {
            let pattern = format!(r"\b{}\b", regex::escape(acronym));
            if let Ok(re) = regex::Regex::new(&pattern) {
                if let Some(expansion) = ACRONYM_MAP.get(acronym) {
                    result = re.replace_all(&result, *expansion).to_string();
                }
            }
        }
        result
    }

    /// Decomposes compound questions into sub-queries by splitting on `?`.
    /// This lets each sub-query retrieve different relevant chunks.
    fn decompose(query: &str) -> Vec<String> {
        let parts: Vec<String> = query
            .split('?')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                let s = s.trim_end_matches('.');
                if !s.ends_with('?') {
                    format!("{}?", s)
                } else {
                    s.to_string()
                }
            })
            .collect();
        if parts.len() <= 1 {
            vec![]
        } else {
            parts
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_single_acronym() {
        assert_eq!(
            QueryTransformer::expand_acronyms("RAG"),
            "Retrieval Augmented Generation"
        );
    }

    #[test]
    fn test_expand_acronym_in_sentence() {
        assert_eq!(
            QueryTransformer::expand_acronyms("how does RAG work"),
            "how does Retrieval Augmented Generation work"
        );
    }

    #[test]
    fn test_no_acronyms() {
        assert_eq!(
            QueryTransformer::expand_acronyms("what is rust ownership"),
            "what is rust ownership"
        );
    }

    #[test]
    fn test_expand_xml() {
        assert_eq!(
            QueryTransformer::expand_acronyms("XML"),
            "eXtensible Markup Language"
        );
    }

    #[test]
    fn test_expand_json() {
        assert_eq!(
            QueryTransformer::expand_acronyms("JSON"),
            "JavaScript Object Notation"
        );
    }

    #[test]
    fn test_multiple_acronyms() {
        let result = QueryTransformer::expand_acronyms("JSON and XML");
        assert!(
            result.contains("JavaScript Object Notation"),
            "expected JSON expansion in '{}'",
            result
        );
        assert!(
            result.contains("eXtensible Markup Language"),
            "expected XML expansion in '{}'",
            result
        );
    }

    #[test]
    fn test_acronym_substring_no_match() {
        // "RAGS" should NOT match "RAG" (word boundary)
        assert_eq!(
            QueryTransformer::expand_acronyms("RAGS"),
            "RAGS"
        );
    }

    #[test]
    fn test_decompose_compound_question() {
        let variants = QueryTransformer::decompose("What is the borrow checker? How does it prevent data races?");
        assert_eq!(variants.len(), 2);
        assert!(variants[0].contains("borrow checker"));
        assert!(variants[1].contains("data races"));
    }

    #[test]
    fn test_single_question_no_decompose() {
        let variants = QueryTransformer::decompose("What is the borrow checker?");
        assert!(variants.is_empty());
    }

    #[test]
    fn test_no_question_mark_no_decompose() {
        let variants = QueryTransformer::decompose("rust borrow checker");
        assert!(variants.is_empty());
    }

    #[test]
    fn test_transform_full_pipeline() {
        let tq = QueryTransformer::transform("How does RAG work? What about BM25?");
        assert!(tq.primary.contains("Retrieval Augmented Generation"));
        assert_eq!(tq.variants.len(), 2);
    }
}
