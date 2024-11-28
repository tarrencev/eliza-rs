use rig::{agent::AgentBuilder, completion::CompletionModel, embeddings::EmbeddingModel};
use tracing::info;

use crate::{character::Character, knowledge::KnowledgeBase};

#[derive(Clone)]
pub struct Agent<M: CompletionModel, E: EmbeddingModel + 'static> {
    pub character: Character,
    completion_model: M,
    knowledge: KnowledgeBase<E>,
}

impl<M: CompletionModel, E: EmbeddingModel> Agent<M, E> {
    pub fn new(character: Character, completion_model: M, knowledge: KnowledgeBase<E>) -> Self {
        info!(name = character.name, "Creating new agent");

        Self {
            character,
            completion_model,
            knowledge,
        }
    }

    pub fn builder(&self) -> AgentBuilder<M> {
        let builder = AgentBuilder::new(self.completion_model.clone())
            .preamble(&self.character.preamble)
            .context(&format!("Your name: {}", self.character.name))
            .dynamic_context(1, self.knowledge.clone().index());

        builder
    }

    pub fn knowledge(&self) -> &KnowledgeBase<E> {
        &self.knowledge
    }
}
