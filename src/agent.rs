use rig::{agent::AgentBuilder, completion::CompletionModel, embeddings::EmbeddingModel};
use tracing::info;

use crate::{character::Character, knowledge::KnowledgeBase};

pub struct Agent<'a, M: CompletionModel, E: EmbeddingModel + 'static> {
    completion_model: M,
    character: Character,
    knowledge_base: Option<&'a KnowledgeBase<E>>,
}

impl<'a, M: CompletionModel, E: EmbeddingModel> Agent<'a, M, E> {
    pub fn new(character: Character, completion_model: M) -> Self {
        info!(name = character.name, "Creating new agent");

        Self {
            completion_model,
            character,
            knowledge_base: None,
        }
    }

    pub fn with_knowledge(mut self, knowledge: &'a KnowledgeBase<E>) -> Self {
        self.knowledge_base = Some(knowledge);
        self
    }

    pub fn builder(&self) -> AgentBuilder<M> {
        let mut builder = AgentBuilder::new(self.completion_model.clone())
            .preamble(&self.character.preamble)
            .context(&format!("Your name: {}", self.character.name));

        if let Some(kb) = self.knowledge_base {
            builder = builder.dynamic_context(1, kb.clone().index());
        }

        builder
    }
}
