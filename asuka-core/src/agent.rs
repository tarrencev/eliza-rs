use rig::{agent::AgentBuilder, completion::CompletionModel, embeddings::EmbeddingModel};
use tracing::info;

use crate::{character::Character, knowledge::KnowledgeBase};

#[derive(Clone)]
pub struct Agent<M: CompletionModel, E: EmbeddingModel + 'static> {
    pub character: Character,
    completion_model: M,
    knowledge_base: Option<KnowledgeBase<E>>,
}

impl<M: CompletionModel, E: EmbeddingModel> Agent<M, E> {
    pub fn new(character: Character, completion_model: M) -> Self {
        info!(name = character.name, "Creating new agent");

        Self {
            character,
            completion_model,
            knowledge_base: None,
        }
    }

    pub fn with_knowledge(mut self, knowledge: KnowledgeBase<E>) -> Self {
        self.knowledge_base = Some(knowledge);
        self
    }

    pub fn builder(&self) -> AgentBuilder<M> {
        let mut builder = AgentBuilder::new(self.completion_model.clone())
            .preamble(&self.character.preamble)
            .context(&format!("Your name: {}", self.character.name));

        if let Some(kb) = self.knowledge_base.clone() {
            builder = builder.dynamic_context(1, kb.clone().index());
        }

        builder
    }
}
