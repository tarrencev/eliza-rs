use rig::embeddings::{DocumentEmbeddings, Embedding, EmbeddingModel};
use rig::vector_store::{VectorStore, VectorStoreError, VectorStoreIndex};
use serde::Deserialize;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use std::path::Path;

pub struct SQLiteVectorStore {
    pool: SqlitePool,
}

impl SQLiteVectorStore {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self, VectorStoreError> {
        let pool = SqlitePoolOptions::new()
            .connect(&format!("sqlite://{}", path.as_ref().display()))
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        // Run migrations or create tables if they don't exist
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                document TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        // ... Similarly for other tables ...

        Ok(Self { pool })
    }

    fn serialize_embedding(embedding: &Embedding) -> String {
        format!(
            "[{}]",
            embedding
                .vec
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl VectorStore for SQLiteVectorStore {
    type Q = String;

    async fn add_documents(
        &mut self,
        documents: Vec<DocumentEmbeddings>,
    ) -> Result<(), VectorStoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        for doc in documents {
            // Store document
            sqlx::query("INSERT OR REPLACE INTO documents (id, document) VALUES (?1, ?2)")
                .bind(&doc.id)
                .bind(&doc.document.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

            // Store embeddings
            for embedding in doc.embeddings {
                sqlx::query("INSERT INTO embeddings (rowid, embedding) VALUES (?1, ?2)")
                    .bind(&doc.id)
                    .bind(&Self::serialize_embedding(&embedding))
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;
        Ok(())
    }

    async fn get_document<T: for<'a> Deserialize<'a>>(
        &self,
        id: &str,
    ) -> Result<Option<T>, VectorStoreError> {
        let result = sqlx::query("SELECT document FROM documents WHERE id = ?1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        match result {
            Some(row) => {
                let doc_str: String = row.get("document");
                let doc = serde_json::from_str(&doc_str)?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    async fn get_document_embeddings(
        &self,
        id: &str,
    ) -> Result<Option<DocumentEmbeddings>, VectorStoreError> {
        // First get the document
        let doc: Option<serde_json::Value> = self.get_document(id).await?;

        if let Some(doc) = doc {
            let embeddings = sqlx::query("SELECT embedding FROM embeddings WHERE rowid = ?1")
                .bind(id)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

            // Parse embeddings
            let embeddings = embeddings
                .into_iter()
                .map(|row| {
                    let e: String = row.get("embedding");
                    let e = e
                        .trim_matches(|c| c == '[' || c == ']')
                        .split(',')
                        .map(|s| s.trim().parse::<f64>().unwrap())
                        .collect::<Vec<_>>();
                    rig::embeddings::Embedding {
                        vec: e.into(),
                        document: "".to_string(), // We don't store individual document chunks
                    }
                })
                .collect();

            Ok(Some(DocumentEmbeddings {
                id: id.to_string(),
                document: doc,
                embeddings,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_document_by_query(
        &self,
        query: Self::Q,
    ) -> Result<Option<DocumentEmbeddings>, VectorStoreError> {
        // This would require the query to be an embedding vector
        let result = sqlx::query(
            "SELECT rowid, distance FROM embeddings 
             WHERE embedding MATCH ?1 
             ORDER BY distance LIMIT 1",
        )
        .bind(&query)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        match result {
            Some(row) => {
                let id: String = row.get("rowid");
                self.get_document_embeddings(&id).await
            }
            None => Ok(None),
        }
    }
}

pub struct SQLiteVectorIndex<M: EmbeddingModel> {
    store: SQLiteVectorStore,
    model: M,
}

impl<M: EmbeddingModel> SQLiteVectorIndex<M> {
    pub fn new(store: SQLiteVectorStore, model: M) -> Self {
        Self { store, model }
    }
}

impl<M: EmbeddingModel + std::marker::Sync> VectorStoreIndex for SQLiteVectorIndex<M> {
    async fn top_n<T: for<'a> Deserialize<'a>>(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String, T)>, VectorStoreError> {
        let embedding = self.model.embed_document(query).await?;
        let query_vec = SQLiteVectorStore::serialize_embedding(&embedding);

        let rows = sqlx::query(
            "SELECT rowid, distance FROM embeddings 
             WHERE embedding MATCH ?1 
             ORDER BY distance 
             LIMIT ?2",
        )
        .bind(&query_vec)
        .bind(n as i64)
        .fetch_all(&self.store.pool)
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        let mut top_n = Vec::new();
        for row in rows {
            let id: String = row.get("rowid");
            let distance: f64 = row.get("distance");
            if let Some(doc) = self.store.get_document(&id).await? {
                top_n.push((distance, id, doc));
            }
        }

        Ok(top_n)
    }

    async fn top_n_ids(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String)>, VectorStoreError> {
        let embedding = self.model.embed_document(query).await?;
        let query_vec = SQLiteVectorStore::serialize_embedding(&embedding);

        let rows = sqlx::query(
            "SELECT rowid, distance FROM embeddings 
             WHERE embedding MATCH ?1 
             ORDER BY distance 
             LIMIT ?2",
        )
        .bind(&query_vec)
        .bind(n as i64)
        .fetch_all(&self.store.pool)
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        let results = rows
            .into_iter()
            .map(|row| {
                let distance: f64 = row.get("distance");
                let id: String = row.get("rowid");
                (distance, id)
            })
            .collect();

        Ok(results)
    }
}
