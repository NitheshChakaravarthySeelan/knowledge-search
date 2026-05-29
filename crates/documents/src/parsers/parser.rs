use crate::models::document::Document;

pub trait DocumentParser: Send + Sync {
    fn parse(&self, document: &Document) -> Result<Document>;
}
