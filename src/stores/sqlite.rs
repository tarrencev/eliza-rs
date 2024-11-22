use rig::embeddings::{DocumentEmbeddings, Embedding, EmbeddingModel};
use rig::vector_store::{VectorStore, VectorStoreError, VectorStoreIndex};
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use sqlite_vec::sqlite3_vec_init;
use std::path::Path;
use tokio_rusqlite::Connection;
use tracing::{debug, info};
use zerocopy::IntoBytes;

#[derive(Clone)]
pub struct SqliteStore {
    conn: Connection,
}

impl SqliteStore {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, VectorStoreError> {
        info!("Initializing SQLite store at {:?}", path.as_ref());
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        let conn = Connection::open(path)
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        debug!("Running initial migrations");
        // Run migrations or create tables if they don't exist
        conn.call(|conn| {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE IF NOT EXISTS documents (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    doc_id TEXT UNIQUE NOT NULL,
                    document TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_doc_id ON documents(doc_id);
                CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(embedding float[1536]);
                COMMIT;",
            )
            .map_err(|e| tokio_rusqlite::Error::from(e))
        })
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(Self { conn })
    }

    fn serialize_embedding(embedding: &Embedding) -> Vec<f32> {
        embedding.vec.iter().map(|x| *x as f32).collect()
    }
}

impl VectorStore for SqliteStore {
    type Q = String;

    async fn add_documents(
        &mut self,
        documents: Vec<DocumentEmbeddings>,
    ) -> Result<(), VectorStoreError> {
        info!("Adding {} documents to store", documents.len());
        self.conn
            .call(|conn| {
                let tx = conn
                    .transaction()
                    .map_err(|e| tokio_rusqlite::Error::from(e))?;

                for doc in documents {
                    debug!("Storing document with id {}", doc.id);
                    // Store document and get auto-incremented ID
                    tx.execute(
                        "INSERT OR REPLACE INTO documents (doc_id, document) VALUES (?1, ?2)",
                        &[&doc.id, &doc.document.to_string()],
                    )
                    .map_err(|e| tokio_rusqlite::Error::from(e))?;

                    let doc_id = tx.last_insert_rowid();

                    // Store embeddings
                    let mut stmt = tx
                        .prepare("INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)")
                        .map_err(|e| tokio_rusqlite::Error::from(e))?;

                    debug!(
                        "Storing {} embeddings for document {}",
                        doc.embeddings.len(),
                        doc.id
                    );
                    for embedding in doc.embeddings {
                        let vec = Self::serialize_embedding(&embedding);
                        let blob = rusqlite::types::Value::Blob(vec.as_slice().as_bytes().to_vec());
                        stmt.execute(rusqlite::params![doc_id, blob])
                            .map_err(|e| tokio_rusqlite::Error::from(e))?;
                    }
                }

                tx.commit().map_err(|e| tokio_rusqlite::Error::from(e))?;
                Ok(())
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(())
    }

    async fn get_document<T: for<'a> Deserialize<'a>>(
        &self,
        id: &str,
    ) -> Result<Option<T>, VectorStoreError> {
        debug!("Fetching document with id {}", id);
        let id_clone = id.to_string();
        let doc_str = self
            .conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT document FROM documents WHERE doc_id = ?1",
                    rusqlite::params![id_clone],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|e| tokio_rusqlite::Error::from(e))
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        match doc_str {
            Some(doc_str) => {
                let doc: T = serde_json::from_str(&doc_str)
                    .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;
                Ok(Some(doc))
            }
            None => {
                debug!("No document found with id {}", id);
                Ok(None)
            }
        }
    }

    async fn get_document_embeddings(
        &self,
        id: &str,
    ) -> Result<Option<DocumentEmbeddings>, VectorStoreError> {
        debug!("Fetching embeddings for document {}", id);
        // First get the document
        let doc: Option<serde_json::Value> = self.get_document(&id).await?;

        if let Some(doc) = doc {
            let id_clone = id.to_string();
            let embeddings = self
                .conn
                .call(move |conn| {
                    let mut stmt = conn.prepare(
                        "SELECT e.embedding 
                         FROM embeddings e
                         JOIN documents d ON e.rowid = d.id
                         WHERE d.doc_id = ?1",
                    )?;

                    let embeddings = stmt
                        .query_map(rusqlite::params![id_clone], |row| {
                            let bytes: Vec<u8> = row.get(0)?;
                            let vec = bytes
                                .chunks(4)
                                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()) as f64)
                                .collect();
                            Ok(rig::embeddings::Embedding {
                                vec,
                                document: "".to_string(),
                            })
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(embeddings)
                })
                .await
                .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

            debug!("Found {} embeddings for document {}", embeddings.len(), id);
            Ok(Some(DocumentEmbeddings {
                id: id.to_string(),
                document: doc,
                embeddings,
            }))
        } else {
            debug!("No embeddings found for document {}", id);
            Ok(None)
        }
    }

    async fn get_document_by_query(
        &self,
        query: Self::Q,
    ) -> Result<Option<DocumentEmbeddings>, VectorStoreError> {
        debug!("Searching for document matching query");
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT d.doc_id, e.distance 
                     FROM embeddings e
                     JOIN documents d ON e.rowid = d.id
                     WHERE e.embedding MATCH ?1  AND k = ?2
                     ORDER BY e.distance",
                )?;

                let result = stmt
                    .query_row(&[query.as_bytes()], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        match result {
            Some((id, distance)) => {
                debug!("Found matching document {} with distance {}", id, distance);
                self.get_document_embeddings(&id).await
            }
            None => {
                debug!("No matching documents found");
                Ok(None)
            }
        }
    }
}

pub struct SqliteVectorIndex<E: EmbeddingModel> {
    store: SqliteStore,
    embedding_model: E,
}

impl<E: EmbeddingModel> SqliteVectorIndex<E> {
    pub fn new(embedding_model: E, store: SqliteStore) -> Self {
        Self {
            store,
            embedding_model,
        }
    }
}

impl<E: EmbeddingModel + std::marker::Sync> VectorStoreIndex for SqliteVectorIndex<E> {
    async fn top_n<T: for<'a> Deserialize<'a>>(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String, T)>, VectorStoreError> {
        debug!("Finding top {} matches for query", n);
        let embedding = self.embedding_model.embed_document(query).await?;
        let query_vec = SqliteStore::serialize_embedding(&embedding);

        let rows = self
            .store
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT d.doc_id, e.distance 
                    FROM embeddings e
                    JOIN documents d ON e.rowid = d.id
                    WHERE e.embedding MATCH ?1 AND k = ?2
                    ORDER BY e.distance",
                )?;

                let rows = stmt
                    .query_map(rusqlite::params![query_vec.as_bytes(), n], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        debug!("Found {} potential matches", rows.len());
        let mut top_n = Vec::new();
        for (id, distance) in rows {
            if let Some(doc) = self.store.get_document(&id).await? {
                top_n.push((distance, id, doc));
            }
        }

        debug!("Returning {} matches", top_n.len());
        Ok(top_n)
    }

    async fn top_n_ids(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String)>, VectorStoreError> {
        debug!("Finding top {} document IDs for query", n);
        let embedding = self.embedding_model.embed_document(query).await?;
        let query_vec = SqliteStore::serialize_embedding(&embedding);

        let results = self
            .store
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT d.doc_id, e.distance 
                     FROM embeddings e
                     JOIN documents d ON e.rowid = d.id
                     WHERE e.embedding MATCH ?1  AND k = ?2
                     ORDER BY e.distance",
                )?;

                let results = stmt
                    .query_map(rusqlite::params![query_vec.as_bytes(), n], |row| {
                        Ok((row.get::<_, f64>(1)?, row.get::<_, String>(0)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(results)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        debug!("Found {} matching document IDs", results.len());
        Ok(results)
    }
}
