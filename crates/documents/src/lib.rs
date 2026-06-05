pub mod chunkers;
pub mod models;
pub mod loaders;
pub mod parsers;

pub use parsers::parser::{Parser, PdfParser, DocxParser, PlainTextParser, ParserRegistry};
