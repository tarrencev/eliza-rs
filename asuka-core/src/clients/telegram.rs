use rig::{
    completion::{CompletionModel, Prompt},
    embeddings::EmbeddingModel,
};
use std::collections::HashSet;
use teloxide::{dispatching::UpdateFilterExt, dptree, prelude::{LoggingErrorHandler, Requester}};
use tracing::{debug, error, info};

use crate::attention::{Attention, AttentionContext};
use crate::knowledge::{ChannelType, IntoKnowledgeMessage, Source};
use crate::{agent::Agent, attention::AttentionCommand};

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

impl IntoKnowledgeMessage for teloxide::types::Message {
    fn into_knowledge_parts(&self) -> (String, String, ChannelType, Source, String) {
        (
            self.id.to_string(),
            self.chat.id.to_string(),
            if self.chat.id.0 == self.clone().from.unwrap().id.0 as i64 {
                ChannelType::DirectMessage
            } else {
                ChannelType::Text
            },
            Source::Telegram,
            self.text().unwrap_or("").to_string(),
        )
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
                    // Cleanly extract channel and message details
                    let channel_id = msg.chat.id.to_string();
                    let _message_id = msg.id.to_string();
                    
                    // Extract sender information
                    let _sender_username = msg.from
                        .as_ref()
                        .and_then(|from| from.username.clone())
                        .unwrap_or_else(|| "unknown".to_string());
                    
                    // Determine channel type
                    let _channel_type = if msg.chat.id.0 == msg.clone().from.unwrap().id.0 as i64 {
                        ChannelType::DirectMessage
                    } else {
                        ChannelType::Text
                    };

                    // Early return if no text content
                    let message_text = match msg.text() {
                        Some(text) => text,
                        None => return Err(eyre::eyre!("No text in message")),
                    };

                    if let Err(err) = knowledge.create_message(&msg).await {
                        error!(?err, "Failed to store message");
                        return Err(eyre::eyre!(err));
                    }

                    debug!("Fetching message history for channel {}", channel_id);
                    let history = match knowledge
                        .channel_messages(&channel_id, MAX_HISTORY_MESSAGES)
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
                                        Some(word[1..].to_string()) // Remove the '@' symbol
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

                    let (_, _, channel_type, source, _) = msg.clone().into_knowledge_parts();

                    let context = AttentionContext {
                        message_content: message_text.to_string(),
                        mentioned_names,
                        history,
                        channel_type,
                        source,
                    };

                    debug!(?context, "Attention context");

                    match attention.should_reply(&context).await {
                        AttentionCommand::Respond => {}
                        _ => {
                            debug!("Bot decided not to reply to message");
                            return Err(eyre::eyre!("Bot decided not to reply to message"));
                        }
                    }

                    let agent =
                        agent
                        .builder()
                        .context(&format!(
                            "Current time: {}",
                            chrono::Local::now().format("%I:%M:%S %p, %Y-%m-%d")
                        ))
                        .context("Please keep your responses concise and under 2000 characters when possible.")
                        .build();

                    let response = match agent.prompt(message_text).await {
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
            .dispatch_with_listener(listener, LoggingErrorHandler::with_custom_text("An error occurred"))
            .await;

        Ok(())
    }
}

pub fn chunk_message(text: &str, max_length: usize, min_chunk_length: usize) -> Vec<String> {
    // Base case: if text is shorter than min_chunk_length, return as single chunk
    if text.len() <= min_chunk_length {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();

    // Find split point for current chunk
    let mut split_index = text.len();
    let mut in_heading = false;

    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if i > 0 && line.starts_with('#') {
            split_index = text.find(line).unwrap_or(text.len());
            in_heading = true;
		}

        // Check if adding this line would exceed max_length
        let line_start = text.find(line).unwrap_or(text.len());
        if line_start + line.len() > max_length && i > 0 {
            split_index = line_start;
            break;
        }
    }

    // Split text and recurse
    if split_index < text.len() {
        let (chunk, rest) = text.split_at(split_index);
        let mut chunk = chunk.trim().to_string();

        // Add newline after chunk if we're not splitting on a heading
        if !in_heading && !rest.trim().starts_with('#') {
            chunk.push('\n');
        }

        // Strip trailing newline if it's the last character
        if chunk.ends_with('\n') {
            chunk.pop();
        }

        chunks.push(chunk);
        chunks.extend(chunk_message(rest.trim(), max_length, min_chunk_length));
    } else {
        chunks.push(text.trim().to_string());
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_message_single_chunk() {
        let text = "This is a short message";
        let chunks = chunk_message(text, 100, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_message_multiple_chunks() {
        let text = "Line 1\nLine 2\nLine 3";
        let chunks = chunk_message(text, 10, 5);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "Line 1");
        assert_eq!(chunks[1], "Line 2");
        assert_eq!(chunks[2], "Line 3");
    }

    #[test]
    fn test_chunk_message_empty_lines() {
        let text = "Line 1\n\n\nLine 2";
        let chunks = chunk_message(text, 100, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Line 1\n\n\nLine 2");
    }

    #[test]
    fn test_chunk_message_markdown() {
        let text = "# Heading 1\nSome text under heading 1\n## Heading 2\nMore text\n# Heading 3\nFinal text";
        let chunks = chunk_message(text, 100, 50);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], "# Heading 1\nSome text under heading 1");
        assert_eq!(
            chunks[1],
            "## Heading 2\nMore text\n# Heading 3\nFinal text"
        );
    }

    #[test]
    fn test_no_chunking_under_min_length() {
        let text = "This is a message that won't be chunked because it's under the minimum length";
        let chunks = chunk_message(text, 10, 1000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }
}
