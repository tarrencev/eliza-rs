use rig::{
    completion::{CompletionModel, Prompt},
    embeddings::EmbeddingModel,
};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::GatewayIntents;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use std::collections::HashSet;
use tracing::{debug, error, info};

use crate::attention::{Attention, AttentionContext};
use crate::knowledge::{ChannelType, IntoKnowledgeMessage, Source};
use crate::{agent::Agent, attention::AttentionCommand};

const MESSAGE_SPLIT: &str = "[MSG_SPLIT]";
const MAX_MESSAGE_LENGTH: usize = 1500;
const MAX_HISTORY_MESSAGES: i64 = 10;

#[derive(Clone)]
pub struct DiscordClient<M: CompletionModel, E: EmbeddingModel + 'static> {
    agent: Agent<M, E>,
    attention: Attention<M>,
}

impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> DiscordClient<M, E> {
    pub fn new(agent: Agent<M, E>, attention: Attention<M>) -> Self {
        Self { agent, attention }
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

impl IntoKnowledgeMessage for serenity::model::channel::Message {
    fn into_knowledge_parts(&self) -> (String, String, ChannelType, Source, String) {
        (
            self.id.to_string(),
            self.channel_id.to_string(),
            if self.guild_id.is_none() {
                ChannelType::DirectMessage
            } else {
                ChannelType::Text
            },
            Source::Discord,
            self.content.clone(),
        )
    }
}

#[async_trait]
impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> EventHandler
    for DiscordClient<M, E>
{
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let knowledge = self.agent.knowledge();

        if let Err(err) = knowledge.create_message(&msg).await {
            error!(?err, "Failed to store message");
            return;
        }

        debug!("Fetching message history for channel {}", msg.channel_id);
        let history = match knowledge
            .channel_messages(&msg.channel_id.to_string(), MAX_HISTORY_MESSAGES)
            .await
        {
            Ok(messages) => {
                debug!(message_count = messages.len(), "Retrieved message history");
                messages
            }
            Err(err) => {
                error!(?err, "Failed to fetch recent messages");
                return;
            }
        };

        let mentioned_names: HashSet<String> =
            msg.mentions.iter().map(|user| user.name.clone()).collect();
        debug!(
            mentioned_names = ?mentioned_names,
            "Mentioned names in message"
        );

        let (_, _, channel_type, source, _) = msg.clone().into_knowledge_parts();

        let context = AttentionContext {
            message_content: msg.content.clone(),
            mentioned_names,
            history,
            channel_type,
            source,
        };

        debug!(?context, "Attention context");

        match self.attention.should_reply(&context).await {
            AttentionCommand::Respond => {}
            _ => {
                debug!("Bot decided not to reply to message");
                return;
            }
        }

        let agent = self
            .agent
            .builder()
            .context(&format!(
                "Current time: {}",
                chrono::Local::now().format("%I:%M:%S %p, %Y-%m-%d")
            ))
            .context(&format!("Discord messages have a {MAX_MESSAGE_LENGTH} character limit. If your response is longer than {MAX_MESSAGE_LENGTH} characters, split it into multiple messages using {MESSAGE_SPLIT} as a separator. Split messages at natural breakpoints like paragraph endings or complete thoughts."))
            .build();

        let response = match agent.prompt(&msg.content).await {
            Ok(response) => response,
            Err(err) => {
                error!(?err, "Failed to generate response");
                return;
            }
        };

        debug!(response = %response, "Generated response");
        for message in response.split(MESSAGE_SPLIT) {
            let message = message.trim();
            if !message.is_empty() {
                if let Err(why) = msg.channel_id.say(&ctx.http, message).await {
                    error!(?why, "Failed to send message");
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!(name = self.agent.character.name, "Bot connected");
        info!(guild_count = ready.guilds.len(), "Serving guilds");
    }
}
