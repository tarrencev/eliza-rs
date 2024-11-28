use tokio_rusqlite::Connection;
use tracing::{debug, info};

use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::VectorStoreError,
    Embed,
};

use crate::stores::sqlite::{SqliteVectorIndex, SqliteVectorStore, SqliteVectorStoreTable};

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

// Core traits for messages
pub trait MessageMetadata {
    fn id(&self) -> String;
    fn source_id(&self) -> String;
    fn channel_id(&self) -> String;
    fn created_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn source(&self) -> Source;
    fn channel_type(&self) -> ChannelType;
}

pub trait MessageContent {
    fn content(&self) -> &str;
}

#[derive(Embed, Clone, Debug)]
pub struct Document {
    pub id: String,
    pub source_id: String,
    #[embed]
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SqliteVectorStoreTable for Document {
    fn name() -> &'static str {
        "documents"
    }

    fn columns() -> Vec<(&'static str, &'static str, bool)> {
        vec![
            ("id", "TEXT PRIMARY KEY", false),
            ("source_id", "TEXT", true),
            ("content", "TEXT", false),
            ("created_at", "TIMESTAMP DEFAULT CURRENT_TIMESTAMP", false),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, String)> {
        vec![
            ("id", self.id.clone()),
            ("source_id", self.source_id.clone()),
            ("content", self.content.clone()),
            ("created_at", self.created_at.to_rfc3339()),
        ]
    }
}

#[derive(Clone, Debug)]
pub enum MessageSource {
    Discord(serenity::model::channel::Message),
    // Add other platform messages here
}

impl MessageMetadata for MessageSource {
    fn id(&self) -> String {
        match self {
            MessageSource::Discord(msg) => msg.id.to_string(),
        }
    }

    fn source_id(&self) -> String {
        match self {
            MessageSource::Discord(msg) => msg.author.id.to_string(),
        }
    }

    fn channel_id(&self) -> String {
        match self {
            MessageSource::Discord(msg) => msg.channel_id.to_string(),
        }
    }

    fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            MessageSource::Discord(msg) => *msg.timestamp,
        }
    }

    fn source(&self) -> Source {
        match self {
            MessageSource::Discord(_) => Source::Discord,
        }
    }

    fn channel_type(&self) -> ChannelType {
        match self {
            MessageSource::Discord(msg) => {
                if msg.guild_id.is_none() {
                    ChannelType::DirectMessage
                } else {
                    ChannelType::Text
                }
            }
        }
    }
}

impl MessageContent for MessageSource {
    fn content(&self) -> &str {
        match self {
            MessageSource::Discord(msg) => &msg.content,
        }
    }
}

#[derive(Embed, Clone, Debug)]
pub struct Message {
    pub source: MessageSource,
    #[embed]
    pub content: String,
}

impl SqliteVectorStoreTable for Message {
    fn name() -> &'static str {
        "messages"
    }

    fn columns() -> Vec<(&'static str, &'static str, bool)> {
        vec![
            ("id", "TEXT PRIMARY KEY", false),
            ("source_id", "TEXT", true),
            ("channel_id", "TEXT", true),
            ("content", "TEXT", false),
            ("created_at", "TIMESTAMP DEFAULT CURRENT_TIMESTAMP", false),
        ]
    }

    fn id(&self) -> String {
        self.source.id()
    }

    fn column_values(&self) -> Vec<(&'static str, String)> {
        vec![
            ("id", self.source.id()),
            ("source_id", self.source.source_id()),
            ("channel_id", self.source.channel_id()),
            ("content", self.content.clone()),
            ("created_at", self.source.created_at().to_rfc3339()),
        ]
    }
}

#[derive(Clone)]
pub struct KnowledgeBase<E: EmbeddingModel + 'static> {
    conn: Connection,
    document_store: SqliteVectorStore<E, Document>,
    message_store: SqliteVectorStore<E, Message>,
    embedding_model: E,
}

impl<E: EmbeddingModel> KnowledgeBase<E> {
    pub async fn new(conn: Connection, embedding_model: E) -> Result<Self, VectorStoreError> {
        let document_store = SqliteVectorStore::new(conn.clone(), &embedding_model).await?;
        let message_store = SqliteVectorStore::new(conn.clone(), &embedding_model).await?;

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

                COMMIT;"
            )
            .map_err(tokio_rusqlite::Error::from)
        })
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(Self {
            conn,
            document_store,
            message_store,
            embedding_model,
        })
    }

    pub async fn create_message(&self, msg: Message) -> anyhow::Result<i64> {
        let embeddings = EmbeddingsBuilder::new(self.embedding_model.clone())
            .documents(vec![msg.clone()])?
            .build()
            .await?;

        let store = self.message_store.clone();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;

                // First upsert the channel
                tx.execute(
                    "INSERT INTO channels (channel_id, channel_type, source) 
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT (channel_id) DO UPDATE SET 
                     updated_at = CURRENT_TIMESTAMP",
                    [
                        &msg.source.channel_id(),
                        msg.source.channel_type().as_str(),
                        msg.source.source().as_str(),
                    ],
                )?;

                let id = store.add_rows_with_txn(&tx, embeddings)?;

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
        I: IntoIterator<Item = Document>,
    {
        info!("Adding documents to KnowledgeBase");
        let embeddings = EmbeddingsBuilder::new(self.embedding_model.clone())
            .documents(documents)?
            .build()
            .await?;

        debug!("Adding embeddings to store");
        self.document_store.add_rows(embeddings).await?;

        info!("Successfully added documents to KnowledgeBase");
        Ok(())
    }

    pub fn document_index(self) -> SqliteVectorIndex<E, Document> {
        SqliteVectorIndex::new(self.embedding_model, self.document_store)
    }

    pub fn message_index(self) -> SqliteVectorIndex<E, Message> {
        SqliteVectorIndex::new(self.embedding_model, self.message_store)
    }
}
