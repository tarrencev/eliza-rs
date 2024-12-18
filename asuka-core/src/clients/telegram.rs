use rig::{
    completion::{CompletionModel, Prompt},
    embeddings::EmbeddingModel,
};
use std::collections::HashSet;
use teloxide::{
    dispatching::UpdateFilterExt,
    dptree,
    prelude::{LoggingErrorHandler, Requester},
};
use tracing::{debug, error, info};

use crate::{agent::Agent, attention::AttentionCommand};
use crate::{
    attention::{Attention, AttentionContext},
    knowledge,
};

const MAX_HISTORY_MESSAGES: i64 = 10;

#[derive(Clone)]
pub struct TelegramClient<M: CompletionModel, E: EmbeddingModel + 'static> {
    agent: Agent<M, E>,
    attention: Attention<M>,
}

impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> TelegramClient<M, E> {
    pub fn new(agent: Agent<M, E>, attention: Attention<M>) -> Self {
        Self { agent, attention }
    }

    pub async fn start(&self, token: &str) -> eyre::Result<()> {
        let bot = teloxide::Bot::new(token);

        info!("Starting telegram bot");

        self.run(bot).await
    }
}

impl From<teloxide::types::Message> for knowledge::Message {
    fn from(msg: teloxide::types::Message) -> Self {
        let user_id = msg
            .from
            .clone()
            .map(|u| u.id.to_string())
            .unwrap_or_default();
        let user_id_num = msg.from.clone().map(|u| u.id.0).unwrap_or_default();

        Self {
            id: msg.id.to_string(),
            source: knowledge::Source::Telegram,
            source_id: user_id.clone(),
            channel_type: if msg.chat.id.0 == user_id_num as i64 {
                knowledge::ChannelType::DirectMessage
            } else {
                knowledge::ChannelType::Text
            },
            channel_id: msg.chat.id.to_string(),
            account_id: user_id,
            role: "user".to_string(),
            content: msg.text().unwrap_or_default().to_string(),
            created_at: msg.date,
        }
    }
}

impl<M: CompletionModel + 'static, E: EmbeddingModel + 'static> TelegramClient<M, E> {
    async fn run(&self, bot: teloxide::Bot) -> eyre::Result<()> {
        let knowledge = self.agent.knowledge().clone();
        let attention = self.attention.clone();
        let agent = self.agent.clone();

        let handler = dptree::entry()
            .branch(teloxide::types::Update::filter_message().endpoint(move |bot: teloxide::Bot, msg: teloxide::types::Message| {
                let knowledge = knowledge.clone();
                let attention = attention.clone();
                let agent = agent.clone();

                async move {
                    let knowledge_msg = knowledge::Message::from(msg.clone());

                    if let Err(err) = knowledge.create_message(knowledge_msg.clone()).await {
                        error!(?err, "Failed to store message");
                        return Err(eyre::eyre!(err));
                    }

                    debug!("Fetching message history for channel {}", msg.chat.id);
                    let history = match knowledge
                        .channel_messages(&msg.chat.id.to_string(), MAX_HISTORY_MESSAGES)
                        .await
                    {
                        Ok(messages) => {
                            debug!(message_count = messages.len(), "Retrieved message history");
                            messages
                        }
                        Err(err) => {
                            error!(?err, "Failed to fetch recent messages");
                            return Err(eyre::eyre!(err));
                        }
                    };

                    let mentioned_names: HashSet<String> = msg.text()
                        .map(|text| {
                            text.split_whitespace()
                                .filter_map(|word| {
                                    if word.starts_with('@') {
                                        Some(word[1..].to_string())
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    debug!(
                        mentioned_names = ?mentioned_names,
                        "Mentioned names in message"
                    );

                    let context = AttentionContext {
                        message_content: msg.text().unwrap_or_default().to_string(),
                        mentioned_names,
                        history,
                        channel_type: knowledge_msg.channel_type,
                        source: knowledge_msg.source,
                    };

                    debug!(?context, "Attention context");

                    match attention.should_reply(&context).await {
                        AttentionCommand::Respond => {}
                        _ => {
                            debug!("Bot decided not to reply to message");
                            return Ok(());
                        }
                    }

                    let agent = agent
                        .builder()
                        .context(&format!(
                            "Current time: {}",
                            chrono::Local::now().format("%I:%M:%S %p, %Y-%m-%d")
                        ))
                        .context("Please keep your responses concise and under 2000 characters when possible.")
                        .build();

                    let response = match agent.prompt(msg.text().unwrap_or_default()).await {
                        Ok(response) => response,
                        Err(err) => {
                            error!(?err, "Failed to generate response");
                            return Err(eyre::eyre!(err));
                        }
                    };

                    debug!(response = %response, "Generated response");

                    if let Err(why) = bot.send_message(msg.chat.id, response).await {
                        error!(?why, "Failed to send message");
                        return Err(eyre::eyre!(why));
                    }

                    Ok(())
                }
            }));

        let listener = teloxide::update_listeners::polling_default(bot.clone()).await;

        teloxide::dispatching::Dispatcher::builder(bot, handler)
            .build()
            .dispatch_with_listener(
                listener,
                LoggingErrorHandler::with_custom_text("Failed to process Telegram update"),
            )
            .await;

        Ok(())
    }
}
