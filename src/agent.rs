use rig::{
    agent::AgentBuilder,
    providers::{self, xai::completion::CompletionModel},
};

use crate::character::Character;

pub struct Agent {
    model: CompletionModel,
    character: Character,
}

impl Agent {
    pub fn new(character: Character, xai_api_key: &str) -> Self {
        let client = providers::xai::Client::new(xai_api_key);
        let model = client.completion_model(providers::xai::GROK_BETA);
        Self { model, character }
    }

    pub fn builder(&self) -> AgentBuilder<CompletionModel> {
        AgentBuilder::new(self.model.clone())
            .preamble(&self.character.preamble)
            .context(&format!("Your name: {}", self.character.name))
    }
}
