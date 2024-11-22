use std::path::PathBuf;

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
        let mut builder = EmbeddingsBuilder::new(self.model.clone());

        for (id, content) in documents {
            builder = builder.simple_document(id.to_str().unwrap(), &content);
        }

        let embeddings = builder.build().await?;
        self.store.add_documents(embeddings).await?;

        Ok(())
    }

    pub fn index(self) -> InMemoryVectorIndex<M> {
        InMemoryVectorIndex::new(self.model, self.store)
    }
}
