use std::path::PathBuf;
use tracing::{debug, info};

use rig::{
    embeddings::{EmbeddingModel, EmbeddingsBuilder},
    vector_store::{
        in_memory_store::{InMemoryVectorIndex, InMemoryVectorStore},
        VectorStore,
    },
};

#[derive(Clone)]
pub struct KnowledgeBase<M: EmbeddingModel> {
    pub store: InMemoryVectorStore,
    model: M,
}

impl<M: EmbeddingModel> KnowledgeBase<M> {
    pub fn new(store: InMemoryVectorStore, model: M) -> Self {
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

    pub fn index(self) -> InMemoryVectorIndex<M> {
        debug!("Creating vector index from KnowledgeBase");
        InMemoryVectorIndex::new(self.model, self.store)
    }
}
