use anyhow::{Result, anyhow};
use std::io::Read;
use pdf_oxide::PdfDocument as PdfDoc;
use dotext::*;

pub trait Parser: Send + Sync {
    /// Supported file extensions (e.g., "pdf", "docx")
    fn supported_extensions(&self) -> Vec<&'static str>;
    
    /// Extract text content from raw bytes
    fn parse(&self, bytes: &[u8]) -> Result<String>;
}

pub struct PdfParser;

impl Parser for PdfParser {
    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["pdf"]
    }

    fn parse(&self, bytes: &[u8]) -> Result<String> {
        let doc = PdfDoc::from_bytes(bytes.to_vec())
            .map_err(|e| anyhow!("Failed to load PDF: {}", e))?;
        
        let mut text = String::new();
        let page_count = doc.page_count()
            .map_err(|e| anyhow!("Failed to get page count: {}", e))?;
            
        for i in 0..page_count {
            if let Ok(content) = doc.extract_text(i) {
                text.push_str(&content);
                text.push('\n');
            }
        }
        
        if text.is_empty() {
            return Err(anyhow!("No text extracted from PDF"));
        }
        
        Ok(text)
    }
}

pub struct DocxParser;

impl Parser for DocxParser {
    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["docx"]
    }

    fn parse(&self, bytes: &[u8]) -> Result<String> {
        // Since dotext only implements `MsDoc::open` taking a file path,
        // we write the bytes to a temp file, parse it, and clean it up.
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(format!("{}.docx", uuid::Uuid::new_v4()));
        std::fs::write(&temp_file_path, bytes)
            .map_err(|e| anyhow!("Failed to write temporary DOCX file: {}", e))?;
        
        let result = (|| {
            let mut docx = Docx::open(&temp_file_path)
                .map_err(|e| anyhow!("Failed to parse DOCX: {}", e))?;
            
            let mut content = String::new();
            docx.read_to_string(&mut content)
                .map_err(|e| anyhow!("Failed to read DOCX content: {}", e))?;
            Ok(content)
        })();
        
        let _ = std::fs::remove_file(temp_file_path);
        result
    }
}

pub struct PlainTextParser;

impl Parser for PlainTextParser {
    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["txt", "md"]
    }

    fn parse(&self, bytes: &[u8]) -> Result<String> {
        String::from_utf8(bytes.to_vec())
            .map_err(|e| anyhow!("Failed to parse plain text: {}", e))
    }
}

/// Factory to select the appropriate parser based on file extension
pub struct ParserRegistry {
    parsers: Vec<Box<dyn Parser>>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self {
            parsers: vec![
                Box::new(PdfParser),
                Box::new(DocxParser),
                Box::new(PlainTextParser),
            ],
        }
    }

    pub fn get_parser(&self, extension: &str) -> Option<&dyn Parser> {
        let ext = extension.to_lowercase();
        self.parsers.iter().find(|p| p.supported_extensions().contains(&ext.as_str())).map(|p| p.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_parser() {
        let parser = PlainTextParser;
        assert_eq!(parser.supported_extensions(), vec!["txt", "md"]);
        
        let bytes = b"Hello world!";
        let parsed = parser.parse(bytes).unwrap();
        assert_eq!(parsed, "Hello world!");
    }

    #[test]
    fn test_parser_registry() {
        let registry = ParserRegistry::new();
        
        let txt_parser = registry.get_parser("txt");
        assert!(txt_parser.is_some());
        assert_eq!(txt_parser.unwrap().supported_extensions(), vec!["txt", "md"]);

        let pdf_parser = registry.get_parser("PDF");
        assert!(pdf_parser.is_some());
        assert_eq!(pdf_parser.unwrap().supported_extensions(), vec!["pdf"]);

        let docx_parser = registry.get_parser("docx");
        assert!(docx_parser.is_some());
        assert_eq!(docx_parser.unwrap().supported_extensions(), vec!["docx"]);

        let unknown_parser = registry.get_parser("unknown");
        assert!(unknown_parser.is_none());
    }

    #[test]
    fn test_pdf_parser_invalid_bytes() {
        let parser = PdfParser;
        let result = parser.parse(b"invalid pdf content");
        assert!(result.is_err());
    }

    #[test]
    fn test_docx_parser_invalid_bytes() {
        let parser = DocxParser;
        let result = parser.parse(b"invalid docx content");
        assert!(result.is_err());
    }
}
