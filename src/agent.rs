use rig::{
    agent::AgentBuilder,
    embeddings::EmbeddingModel,
    providers::{self, xai::completion::CompletionModel},
};

use crate::{character::Character, knowledge::KnowledgeBase};

pub struct Agent<'a, M: EmbeddingModel + 'static> {
    model: CompletionModel,
    character: Character,
    knowledge_base: Option<&'a KnowledgeBase<M>>,
}

impl<'a, M: EmbeddingModel> Agent<'a, M> {
    pub fn new(character: Character, xai_api_key: &str) -> Self {
        let client = providers::xai::Client::new(xai_api_key);
        let model = client.completion_model(providers::xai::GROK_BETA);

        Self {
            model,
            character,
            knowledge_base: None,
        }
    }

    pub fn with_knowledge(mut self, knowledge: &'a KnowledgeBase<M>) -> Self {
        self.knowledge_base = Some(knowledge);
        self
    }

    pub fn builder(&self) -> AgentBuilder<CompletionModel> {
        let mut builder = AgentBuilder::new(self.model.clone())
            .preamble(&self.character.preamble)
            .context(&format!("Your name: {}", self.character.name));

        if let Some(kb) = self.knowledge_base {
            builder = builder.dynamic_context(1, kb.clone().index());
        }

        builder
    }
}
