use async_trait::async_trait;
use std::sync::Arc;
use common::errors::Result;
use common::types::TenantId;
use search::retrievers::Retriever;
use crate::LlmProvider;

pub struct RagService {
    retriever: Arc<dyn Retriever>,
    llm_provider: Arc<dyn LlmProvider>,
}

impl RagService {
    pub fn new(retriever: Arc<dyn Retriever>, llm_provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            retriever,
            llm_provider,
        }
    }

    pub async fn ask(&self, tenant_id: &TenantId, question: &str) -> Result<String> {
        // 1. Retrieve relevant context
        let search_results = self.retriever.retrieve(tenant_id, question, 5).await?;
        
        if search_results.is_empty() {
            return Ok("I'm sorry, I couldn't find any information in your knowledge base to answer that question.".to_string());
        }

        // 2. Construct context string
        let context = search_results
            .iter()
            .map(|res| format!("Source: {}\nContent: {}", res.document_id, res.content))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        // 3. Define System Instruction (The "Personality" of the RAG engine)
        let system_instruction = r#"
            You are Knowledge-OS, an enterprise AI assistant. 
            Use the provided context to answer the user's question accurately.
            If the answer is not in the context, say that you don't know based on the current knowledge base.
            Keep your answer professional, concise, and structured using Markdown.
        "#;

        // 4. Construct Prompt
        let prompt = format!(
            "Context from knowledge base:\n{}\n\nUser Question: {}\n\nAnswer:",
            context, question
        );

        // 5. Generate Answer
        self.llm_provider.generate(&prompt, Some(system_instruction)).await
    }
}
