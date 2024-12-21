use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::VectorStoreError,
};
use tokio_rusqlite::Connection;
use tracing::{debug, info};

use super::models::{Account, Channel, Document, Message};
use rig_sqlite::{SqliteError, SqliteVectorIndex, SqliteVectorStore};
use rusqlite::OptionalExtension;

#[derive(Clone)]
pub struct KnowledgeBase<E: EmbeddingModel + Clone + 'static> {
    pub conn: Connection,
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

    pub async fn create_user(
        &self,
        name: String,
        source: String,
        source_id: String,
    ) -> Result<i64, SqliteError> {
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "INSERT INTO accounts (name, source, created_at, updated_at, source_id)
                 VALUES (?1, ?2, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, ?3)
                 ON CONFLICT(source_id) DO UPDATE SET 
                     updated_at = CURRENT_TIMESTAMP
                 RETURNING id",
                    rusqlite::params![name, source, source_id],
                    |row| row.get(0),
                )
                .map_err(tokio_rusqlite::Error::from)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub fn document_index(&self) -> SqliteVectorIndex<E, Document> {
        SqliteVectorIndex::new(self.embedding_model.clone(), self.document_store.clone())
    }

    pub fn message_index(&self) -> SqliteVectorIndex<E, Message> {
        SqliteVectorIndex::new(self.embedding_model.clone(), self.message_store.clone())
    }

    pub async fn get_user_by_source(&self, source: String) -> Result<Option<Account>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT source_id, name, source, created_at, updated_at FROM accounts WHERE source = ?1"
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
        source: String,
    ) -> Result<i64, SqliteError> {
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "INSERT INTO channels (channel_id, channel_type, source, name, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
                 ON CONFLICT(channel_id) DO UPDATE SET 
                     name = COALESCE(?4, name),
                     updated_at = CURRENT_TIMESTAMP
                 RETURNING id",
                    rusqlite::params![channel_id, channel_type, source, name],
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
                    "SELECT id, channel_id, channel_type, source, name, created_at, updated_at FROM channels WHERE id = ?1",
                )?;

                let channel = stmt
                    .query_row(rusqlite::params![id], |row| Channel::try_from(row))
                    .optional()?;

                Ok(channel)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_channel_by_channel_id(
        &self,
        channel_id: &str,
        source: &str,
    ) -> Result<Option<Channel>, SqliteError> {
        let channel_id = channel_id.to_string();
        let source = source.to_string();

        self.conn
        .call(move |conn| {
            let result = conn.prepare("SELECT id, name, source, created_at, updated_at FROM channels WHERE channel_id = ?1 AND source = ?2")?
                .query_row(rusqlite::params![channel_id, source], |row| {
                        Channel::try_from(row)
                    })
                    .optional()?;

                Ok(result)
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
                    Channel::try_from(row)
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

                tx.execute(
                    "INSERT INTO messages (id, channel_id, account_id, content, role, created_at) 
                 VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
                 ON CONFLICT (id) DO UPDATE SET 
                     channel_id = ?2, 
                     account_id = ?3, 
                     content = ?4, 
                     role = ?5, 
                     created_at = CURRENT_TIMESTAMP",
                    [
                        &msg.id,
                        &msg.channel_id,
                        &msg.account_id,
                        &msg.content,
                        &msg.role,
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
                    }).optional()?)
            })
            .await
            .map_err(|e| SqliteError::DatabaseError(Box::new(e)))
    }

    pub async fn get_recent_messages_in_channel(
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

    pub async fn get_recent_messages(&self, limit: usize) -> Result<Vec<Message>, SqliteError> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, source, source_id, channel_type, channel_id, account_id, role, content, created_at 
                     FROM messages 
                     ORDER BY created_at DESC 
                     LIMIT ?1",
                )?;

                let messages = stmt
                    .query_map(rusqlite::params![limit], |row| {
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

        debug!("Adding embeddings to document store");
        self.document_store.add_rows(embeddings).await?;

        info!("Successfully added documents to KnowledgeBase");
        Ok(())
    }
}
