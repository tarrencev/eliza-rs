use std::path::PathBuf;
use tokio_rusqlite::Connection;
use tracing::{debug, info};

use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::{VectorStore, VectorStoreError},
};

use crate::stores::sqlite::{SqliteVectorIndex, SqliteVectorStore};

// Define enums at module level
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Discord,
    Telegram,
    Github,
    X,
    Twitter,
}

impl Source {
    pub fn as_str(&self) -> &'static str {
        match self {
            Source::Discord => "discord",
            Source::Telegram => "telegram",
            Source::Github => "github",
            Source::X => "x",
            Source::Twitter => "twitter",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "discord" => Some(Source::Discord),
            "telegram" => Some(Source::Telegram),
            "github" => Some(Source::Github),
            "x" => Some(Source::X),
            "twitter" => Some(Source::Twitter),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelType {
    DirectMessage,
    Text,
    Voice,
    Thread,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::DirectMessage => "direct_message",
            ChannelType::Text => "text",
            ChannelType::Voice => "voice",
            ChannelType::Thread => "thread",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "direct_message" => Some(ChannelType::DirectMessage),
            "text" => Some(ChannelType::Text),
            "voice" => Some(ChannelType::Voice),
            "thread" => Some(ChannelType::Thread),
            _ => None,
        }
    }
}

pub trait IntoKnowledgeMessage {
    fn into_knowledge_parts(&self) -> (String, String, ChannelType, Source, String);
}

#[derive(Debug)]
pub enum Message {
    Discord(serenity::model::channel::Message),
}

#[derive(Clone)]
pub struct KnowledgeBase<E: EmbeddingModel> {
    conn: Connection,
    store: SqliteVectorStore<E>,
    embedding_model: E,
}

impl<E: EmbeddingModel> KnowledgeBase<E> {
    pub async fn new(conn: Connection, embedding_model: E) -> Result<Self, VectorStoreError> {
        let store = SqliteVectorStore::new(conn.clone(), &embedding_model).await?;

        conn.call(|conn| {
            conn.execute_batch(
                "BEGIN;

                -- User management tables
                CREATE TABLE IF NOT EXISTS accounts (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_id TEXT NOT NULL,
                    source TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                );
                CREATE INDEX IF NOT EXISTS idx_source_id_source ON accounts(source_id, source);

                -- Channel tables
                CREATE TABLE IF NOT EXISTS channels (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    channel_id TEXT UNIQUE NOT NULL,
                    channel_type TEXT NOT NULL,
                    source TEXT NOT NULL,
                    name TEXT,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                );
                CREATE INDEX IF NOT EXISTS idx_channel_id_type ON channels(channel_id, channel_type);

                -- Messages table
                CREATE TABLE IF NOT EXISTS messages (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    source_id TEXT NOT NULL,
                    channel_id TEXT NOT NULL,
                    content TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                );
                CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id);
                CREATE INDEX IF NOT EXISTS idx_messages_source ON messages(source_id);

                COMMIT;"
            )
            .map_err(tokio_rusqlite::Error::from)
        })
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(Self {
            conn,
            store,
            embedding_model,
        })
    }

    pub async fn create_message<T: IntoKnowledgeMessage>(&self, msg: &T) -> anyhow::Result<i64> {
        let (source_id, channel_id, channel_type, source, content) = msg.into_knowledge_parts();
        let content = content.to_string();
        let source_id = source_id.to_string();
        let channel_id = channel_id.to_string();
        let source_str = source.as_str();
        let channel_type_str = channel_type.as_str();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                // First upsert the channel
                tx.execute(
                    "INSERT INTO channels (channel_id, channel_type, source) 
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT (channel_id) DO UPDATE SET 
                     updated_at = CURRENT_TIMESTAMP",
                    [&channel_id, channel_type_str, source_str],
                )?;

                // Then create the message
                let id = {
                    let mut stmt = tx.prepare(
                        "INSERT INTO messages (source_id, channel_id, content) 
                         VALUES (?1, ?2, ?3) 
                         RETURNING id",
                    )?;

                    stmt.query_row([&source_id, &channel_id, &content], |row| row.get(0))?
                };

                tx.commit()?;

                Ok(id)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn channel_messages(
        &self,
        channel_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let channel_id = channel_id.to_string();

        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT source_id, content 
                     FROM messages 
                     WHERE channel_id = ?1
                     ORDER BY created_at DESC 
                     LIMIT ?2",
                )?;
                let messages = stmt
                    .query_map([&channel_id, &limit.to_string()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(messages)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn add_documents<'a, I>(&mut self, documents: I) -> anyhow::Result<()>
    where
        I: IntoIterator<Item = (PathBuf, String)>,
    {
        info!("Adding documents to KnowledgeBase");
        let mut builder = EmbeddingsBuilder::new(self.embedding_model.clone());

        for (id, content) in documents {
            let id_str = id.to_str().unwrap();
            debug!(document_id = id_str, "Adding document");
            builder = builder.simple_document(id_str, &content);
        }

        debug!("Building embeddings");
        let embeddings = builder.build().await?;

        debug!("Adding embeddings to store");
        self.store.add_documents(embeddings).await?;

        info!("Successfully added documents to KnowledgeBase");
        Ok(())
    }

    pub fn index(self) -> SqliteVectorIndex<E> {
        SqliteVectorIndex::new(self.embedding_model, self.store)
    }
}
