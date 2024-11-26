use std::path::PathBuf;
use tracing::{debug, info};

use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::VectorStore,
};

use crate::stores::sqlite::{SqliteVectorIndex, SqliteVectorStore};

#[derive(Clone)]
pub struct KnowledgeBase<M: EmbeddingModel> {
    pub store: SqliteVectorStore<M>,
    model: M,
}

impl<M: EmbeddingModel> KnowledgeBase<M> {
    pub fn new(store: SqliteVectorStore<M>, model: M) -> Self {
        Self { store, model }
    }

    pub async fn add_documents<'a, I>(&mut self, documents: I) -> anyhow::Result<()>
    where
        I: IntoIterator<Item = (PathBuf, String)>,
    {
        info!("Adding documents to KnowledgeBase");
        let mut builder = EmbeddingsBuilder::new(self.model.clone());

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

    pub fn index(self) -> SqliteVectorIndex<M> {
        SqliteVectorIndex::new(self.model, self.store)
    }
}
