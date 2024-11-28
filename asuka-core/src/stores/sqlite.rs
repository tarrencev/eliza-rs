use rig::embeddings::{Embedding, EmbeddingModel};
use rig::vector_store::{VectorStoreError, VectorStoreIndex};
use rig::OneOrMany;
use serde::Deserialize;
use std::marker::PhantomData;
use tokio_rusqlite::Connection;
use tracing::{debug, info};
use zerocopy::IntoBytes;

#[derive(Debug)]
pub enum SqliteError {
    DatabaseError(Box<dyn std::error::Error + Send + Sync>),
    SerializationError(Box<dyn std::error::Error + Send + Sync>),
}

pub trait SqliteVectorStoreTable: Send + Sync + Clone {
    /// Name of the table to store this document type
    fn name() -> &'static str;

    /// Additional columns to add to the table schema beyond the default ones
    /// Returns Vec of (column_name, column_type, should_index)
    fn columns() -> Vec<(&'static str, &'static str, bool)>;

    /// Get the document ID
    fn id(&self) -> String;

    /// Get values for additional columns defined in SqliteVectorStoreTable
    /// Returns Vec of (column_name, value)
    fn column_values(&self) -> Vec<(&'static str, String)>;
}

#[derive(Clone)]
pub struct SqliteVectorStore<E: EmbeddingModel + 'static, T: SqliteVectorStoreTable + 'static> {
    conn: Connection,
    _phantom: PhantomData<(E, T)>,
}

impl<E: EmbeddingModel + 'static, T: SqliteVectorStoreTable + 'static> SqliteVectorStore<E, T> {
    pub async fn new(conn: Connection, embedding_model: &E) -> Result<Self, VectorStoreError> {
        let dims = embedding_model.ndims();
        let table_name = T::name();
        let additional_columns = T::columns();

        // Build the table schema
        let mut create_table = format!("CREATE TABLE IF NOT EXISTS {} (", table_name);

        // Add additional columns
        let mut first = true;
        for (col_name, col_type, _) in &additional_columns {
            if !first {
                create_table.push_str(",");
            }
            create_table.push_str(&format!("\n    {} {}", col_name, col_type));
            first = false;
        }

        // Close parenthesis
        create_table.push_str("\n)");

        // Build index creation statements
        let mut create_indexes = vec![format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_id ON {}(id)",
            table_name, table_name
        )];

        // Add indexes for marked columns
        for (col_name, _, should_index) in additional_columns {
            if should_index {
                create_indexes.push(format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_{} ON {}({})",
                    table_name, col_name, table_name, col_name
                ));
            }
        }

        conn.call(move |conn| {
            conn.execute_batch("BEGIN")?;

            // Create document table
            conn.execute_batch(&create_table)?;

            // Create indexes
            for index_stmt in create_indexes {
                conn.execute_batch(&index_stmt)?;
            }

            // Create embeddings table
            conn.execute_batch(&format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS {}_embeddings USING vec0(embedding float[{}])",
                table_name, dims
            ))?;

            conn.execute_batch("COMMIT")?;
            Ok(())
        })
        .await
        .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        Ok(Self {
            conn,
            _phantom: PhantomData,
        })
    }

    pub fn index(self, model: E) -> SqliteVectorIndex<E, T> {
        SqliteVectorIndex::new(model, self)
    }

    pub fn add_rows_with_txn(
        &self,
        txn: &rusqlite::Transaction<'_>,
        documents: Vec<(T, OneOrMany<Embedding>)>,
    ) -> Result<i64, tokio_rusqlite::Error> {
        info!("Adding {} documents to store", documents.len());
        let table_name = T::name();
        let mut last_id = 0;

        for (doc, embeddings) in &documents {
            debug!("Storing document with id {}", doc.id());

            let mut values = vec![("id", doc.id())];
            values.extend(doc.column_values());

            let columns = values.iter().map(|(col, _)| *col).collect::<Vec<_>>();
            let placeholders = (1..=values.len())
                .map(|i| format!("?{}", i))
                .collect::<Vec<_>>();

            txn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
                    table_name,
                    columns.join(", "),
                    placeholders.join(", ")
                ),
                rusqlite::params_from_iter(values.iter().map(|(_, val)| val)),
            )?;
            last_id = txn.last_insert_rowid();

            let mut stmt = txn.prepare(&format!(
                "INSERT INTO {}_embeddings (rowid, embedding) VALUES (?1, ?2)",
                table_name
            ))?;
            debug!(
                "Storing {} embeddings for document {}",
                embeddings.len(),
                doc.id()
            );
            for embedding in embeddings.iter() {
                let vec = serialize_embedding(&embedding);
                let blob = rusqlite::types::Value::Blob(vec.as_bytes().to_vec());
                stmt.execute(rusqlite::params![last_id, blob])?;
            }
        }

        Ok(last_id)
    }

    pub async fn add_rows(
        &self,
        documents: Vec<(T, OneOrMany<Embedding>)>,
    ) -> Result<i64, VectorStoreError> {
        let documents = documents.clone();
        let this = self.clone();

        self.conn
            .call(move |conn| {
                let tx = conn.transaction().map_err(tokio_rusqlite::Error::from)?;
                let result = this.add_rows_with_txn(&tx, documents)?;
                tx.commit().map_err(tokio_rusqlite::Error::from)?;
                Ok(result)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))
    }
}

pub struct SqliteVectorIndex<E: EmbeddingModel + 'static, T: SqliteVectorStoreTable + 'static> {
    store: SqliteVectorStore<E, T>,
    embedding_model: E,
}

impl<E: EmbeddingModel + 'static, T: SqliteVectorStoreTable> SqliteVectorIndex<E, T> {
    pub fn new(embedding_model: E, store: SqliteVectorStore<E, T>) -> Self {
        Self {
            store,
            embedding_model,
        }
    }
}

impl<E: EmbeddingModel + std::marker::Sync, T: SqliteVectorStoreTable> VectorStoreIndex
    for SqliteVectorIndex<E, T>
{
    async fn top_n<D: for<'a> Deserialize<'a>>(
        &self,
        query: &str,
        n: usize,
    ) -> Result<Vec<(f64, String, D)>, VectorStoreError> {
        debug!("Finding top {} matches for query", n);
        let embedding = self.embedding_model.embed_text(query).await?;
        let query_vec: Vec<f32> = serialize_embedding(&embedding);
        let table_name = T::name();

        // Get all column names from SqliteVectorStoreTable
        let columns = T::columns();
        let column_names: Vec<&str> = columns.iter().map(|(name, _, _)| *name).collect();

        let rows = self
            .store
            .conn
            .call(move |conn| {
                // Build SELECT statement with all columns
                let select_cols = column_names.join(", ");
                let mut stmt = conn.prepare(&format!(
                    "SELECT d.{}, e.distance 
                    FROM {}_embeddings e
                    JOIN {} d ON e.rowid = d.id
                    WHERE e.embedding MATCH ?1 AND k = ?2
                    ORDER BY e.distance",
                    select_cols, table_name, table_name
                ))?;

                let rows = stmt
                    .query_map(rusqlite::params![query_vec.as_bytes().to_vec(), n], |row| {
                        // Create a map of column names to values
                        let mut map = serde_json::Map::new();
                        for (i, col_name) in column_names.iter().enumerate() {
                            let value: String = row.get(i)?;
                            map.insert(col_name.to_string(), serde_json::Value::String(value));
                        }
                        let distance: f64 = row.get(column_names.len())?;
                        let id: String = row.get(0)?; // Assuming id is always first column

                        Ok((id, serde_json::Value::Object(map), distance))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        debug!("Found {} potential matches", rows.len());
        let mut top_n = Vec::new();
        for (id, doc_value, distance) in rows {
            match serde_json::from_value::<D>(doc_value) {
                Ok(doc) => {
                    top_n.push((distance, id, doc));
                }
                Err(e) => {
                    debug!("Failed to deserialize document {}: {}", id, e);
                    continue;
                }
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
        let embedding = self.embedding_model.embed_text(query).await?;
        let query_vec = serialize_embedding(&embedding);
        let table_name = T::name();

        let results = self
            .store
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(&format!(
                    "SELECT d.id, e.distance 
                     FROM {0}_embeddings e
                     JOIN {0} d ON e.rowid = d.id
                     WHERE e.embedding MATCH ?1 AND k = ?2
                     ORDER BY e.distance",
                    table_name
                ))?;

                let results = stmt
                    .query_map(
                        rusqlite::params![
                            query_vec
                                .iter()
                                .flat_map(|x| x.to_le_bytes())
                                .collect::<Vec<u8>>(),
                            n
                        ],
                        |row| Ok((row.get::<_, f64>(1)?, row.get::<_, String>(0)?)),
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(results)
            })
            .await
            .map_err(|e| VectorStoreError::DatastoreError(Box::new(e)))?;

        debug!("Found {} matching document IDs", results.len());
        Ok(results)
    }
}

fn serialize_embedding(embedding: &Embedding) -> Vec<f32> {
    embedding.vec.iter().map(|x| *x as f32).collect()
}
