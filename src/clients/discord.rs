use rig::{
    completion::{CompletionModel, Prompt},
    embeddings::EmbeddingModel,
};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use tracing::{error, info};

#[derive(Clone)]
pub struct DiscordClient<M: CompletionModel, E: EmbeddingModel + 'static> {
    agent: Agent<M, E>,
}

use serenity::model::gateway::GatewayIntents;

use crate::agent::Agent;

impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> DiscordClient<M, E> {
    pub fn new(agent: Agent<M, E>) -> Self {
        Self { agent }
    }

    pub async fn start(&self, token: &str) -> Result<(), serenity::Error> {
        let intents = GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT;

        let mut client = Client::builder(token, intents)
            .event_handler(self.clone())
            .await?;

        info!("Starting discord bot");
        client.start().await
    }
}

#[async_trait]
impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> EventHandler
    for DiscordClient<M, E>
{
    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore messages from bots to prevent potential loops
        if msg.author.bot {
            return;
        }

        let agent = self
            .agent
            .builder()
            .context(&format!(
                "Current time: {}",
                chrono::Local::now().format("%I:%M:%S %p, %Y-%m-%d")
            ))
            .build();

        let response = match agent.prompt(&msg.content).await {
            Ok(response) => response,
            Err(err) => {
                error!(?err, "Failed to generate response");
                return;
            }
        };

        if let Err(why) = msg.channel_id.say(&ctx.http, response).await {
            error!(?why, "Failed to send message");
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!(name = self.agent.character.name, "Bot connected");
        info!(guild_count = ready.guilds.len(), "Serving guilds");
    }
}
