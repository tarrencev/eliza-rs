use rusqlite::OptionalExtension;
use serde::Deserialize;
use tokio_rusqlite::Connection;
use tracing::{debug, info};

use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::VectorStoreError,
    Embed,
};

use crate::stores::sqlite::{
    Column, ColumnValue, SqliteError, SqliteVectorIndex, SqliteVectorStore, SqliteVectorStoreTable,
};

// Define enums at module level
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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

    fn schema() -> Vec<Column> {
        vec![
            Column::new("id", "TEXT PRIMARY KEY"),
            Column::new("source_id", "TEXT").indexed(),
            Column::new("content", "TEXT"),
            Column::new("created_at", "TIMESTAMP DEFAULT CURRENT_TIMESTAMP"),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, Box<dyn ColumnValue>)> {
        vec![
            ("id", Box::new(self.id.clone())),
            ("source_id", Box::new(self.source_id.clone())),
            ("content", Box::new(self.content.clone())),
            ("created_at", Box::new(self.created_at.to_rfc3339())),
        ]
    }
}

#[derive(Debug, Deserialize)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Embed, Clone, Debug, Deserialize)]
pub struct Message {
    pub id: String,
    pub source: Source,
    pub source_id: String,
    pub channel_type: ChannelType,
    pub channel_id: String,
    pub account_id: String,
    pub role: String,
    #[embed]
    pub content: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SqliteVectorStoreTable for Message {
    fn name() -> &'static str {
        "messages"
    }

    fn schema() -> Vec<Column> {
        vec![
            Column::new("id", "TEXT PRIMARY KEY"),
            Column::new("source", "TEXT"),
            Column::new("source_id", "TEXT").indexed(),
            Column::new("channel_type", "TEXT"),
            Column::new("channel_id", "TEXT").indexed(),
            Column::new("account_id", "TEXT").indexed(),
            Column::new("role", "TEXT"),
            Column::new("content", "TEXT"),
            Column::new("created_at", "TIMESTAMP DEFAULT CURRENT_TIMESTAMP"),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, Box<dyn ColumnValue>)> {
        vec![
            ("id", Box::new(self.id.clone())),
            ("source", Box::new(self.source.as_str().to_string())),
            ("source_id", Box::new(self.source_id.clone())),
            (
                "channel_type",
                Box::new(self.channel_type.as_str().to_string()),
            ),
            ("channel_id", Box::new(self.channel_id.clone())),
            ("account_id", Box::new(self.account_id.clone())),
            ("role", Box::new(self.role.clone())),
            ("content", Box::new(self.content.clone())),
            ("created_at", Box::new(self.created_at.to_rfc3339())),
        ]
    }
}

#[derive(Debug)]
struct ConversionError(String);

impl std::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConversionError {}

impl TryFrom<&rusqlite::Row<'_>> for Message {
    type Error = rusqlite::Error;

    fn try_from(row: &rusqlite::Row) -> Result<Self, Self::Error> {
        Ok(Message {
            id: row.get(0)?,
            source: Source::from_str(&row.get::<_, String>(1)?).ok_or(
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(ConversionError("Invalid source".to_string())),
                ),
            )?,
            source_id: row.get(2)?,
            channel_type: ChannelType::from_str(&row.get::<_, String>(3)?).ok_or(
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(ConversionError("Invalid channel type".to_string())),
                ),
            )?,
            channel_id: row.get(4)?,
            account_id: row.get(5)?,
            role: row.get(6)?,
            content: row.get(7)?,
            created_at: row.get(8)?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
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
                    name TEXT NOT NULL,
                    source_id TEXT NOT NULL UNIQUE,
                    source TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                );
                CREATE INDEX IF NOT EXISTS idx_source_id_source ON accounts(source_id, source);

                -- Channel tables
                CREATE TABLE IF NOT EXISTS channels (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    channel_id TEXT NOT NULL UNIQUE,
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

    pub async fn create_user(&self, name: String, source: String) -> Result<i64, SqliteError> {
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "INSERT INTO accounts (name, source, created_at, updated_at)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(name) DO UPDATE SET 
                     updated_at = CURRENT_TIMESTAMP
                 RETURNING id",
                    rusqlite::params![name, source],
                    |row| row.get(0),
                )
                .map_err(tokio_rusqlite::Error::from)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_user_by_source(&self, source: String) -> Result<Option<Account>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, source, created_at, updated_at FROM accounts WHERE source = ?1"
                )?;

                let account = stmt.query_row(rusqlite::params![source], |row| {
                    Ok(Account {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        source: row.get(2)?,
                        created_at: row.get::<_, String>(3)?.parse().unwrap(),
                        updated_at: row.get::<_, String>(4)?.parse().unwrap(),
                    })
                }).optional()?;

                Ok(account)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn create_channel(
        &self,
        channel_id: String,
        channel_type: String,
        name: Option<String>,
    ) -> Result<i64, SqliteError> {
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "INSERT INTO channels (channel_id, channel_type, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(channel_id) DO UPDATE SET 
                     name = COALESCE(?3, name),
                     updated_at = CURRENT_TIMESTAMP
                 RETURNING id",
                    rusqlite::params![channel_id, channel_type, name],
                    |row| row.get(0),
                )
                .map_err(tokio_rusqlite::Error::from)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_channel(&self, id: i64) -> Result<Option<Channel>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, source, created_at, updated_at FROM channels WHERE id = ?1",
                )?;

                let channel = stmt
                    .query_row(rusqlite::params![id], |row| {
                        Ok(Channel {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            source: row.get(2)?,
                            created_at: row.get::<_, String>(3)?.parse().unwrap(),
                            updated_at: row.get::<_, String>(4)?.parse().unwrap(),
                        })
                    })
                    .optional()?;

                Ok(channel)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_channels_by_source(
        &self,
        source: String,
    ) -> Result<Vec<Channel>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, source, created_at, updated_at FROM channels WHERE source = ?1"
                )?;

                let channels = stmt.query_map(rusqlite::params![source], |row| {
                    Ok(Channel {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        source: row.get(2)?,
                        created_at: row.get::<_, String>(3)?.parse().unwrap(),
                        updated_at: row.get::<_, String>(4)?.parse().unwrap(),
                    })
                }).and_then(|mapped_rows| {
                    mapped_rows.collect::<Result<Vec<Channel>, _>>()
                })?;

                Ok(channels)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
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
                    "INSERT INTO channels (channel_id, channel_type, source, name, created_at, updated_at) 
                     VALUES (?1, ?2, ?3, NULL, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                     ON CONFLICT (channel_id) DO UPDATE SET 
                     updated_at = CURRENT_TIMESTAMP",
                    [
                        &msg.channel_id,
                        &msg.channel_type.as_str().to_string(), 
                        &msg.source.as_str().to_string(),
                    ],
                )?;

                let id = store.add_rows_with_txn(&tx, embeddings)?;

                tx.commit()?;

                Ok(id)
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn get_message(&self, id: i64) -> Result<Option<Message>, SqliteError> {
        self.conn
            .call(move |conn| {
                Ok(conn.prepare("SELECT id, source, source_id, channel_type, channel_id, account_id, role, content, created_at FROM messages WHERE id = ?1")?
                    .query_row(rusqlite::params![id], |row| {
                        Message::try_from(row)
                    }).optional().unwrap())
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_recent_messages(
        &self,
        channel_id: i64,
        limit: usize,
    ) -> Result<Vec<Message>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source, source_id, channel_type, channel_id, account_id, role, content, created_at 
                     FROM messages 
                     WHERE channel_id = ?1 
                     ORDER BY created_at DESC 
                     LIMIT ?2",
                )?;

                let messages = stmt
                    .query_map(rusqlite::params![channel_id, limit], |row| {
                        Message::try_from(row)
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(messages)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
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
