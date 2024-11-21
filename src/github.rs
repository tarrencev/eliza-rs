use crate::embeddings::{EmbeddingClient, VectorStore};
use anyhow::Result;
use git2::Repository;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use walkdir::WalkDir;

pub struct GitHubManager<E, V>
where
    E: EmbeddingClient + Send + Sync,
    V: VectorStore + Send + Sync,
{
    embedding_client: Arc<E>,
    vector_store: Arc<V>,
    repo_path: PathBuf,
}

impl<E, V> GitHubManager<E, V>
where
    E: EmbeddingClient + Send + Sync,
    V: VectorStore + Send + Sync,
{
    pub fn new(embedding_client: Arc<E>, vector_store: Arc<V>) -> Self {
        Self {
            embedding_client,
            vector_store,
            repo_path: PathBuf::from("repos"),
        }
    }

    pub fn clone_repository(&self, repo_url: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.repo_path)?;

        let repo_name = repo_url
            .split('/')
            .last()
            .unwrap_or("repo")
            .replace(".git", "");

        let clone_path = self.repo_path.join(&repo_name);
        println!("Cloning repository to {:?}", clone_path);
        Repository::clone(repo_url, &clone_path)?;

        Ok(clone_path)
    }

    fn is_ignored(path: &Path) -> bool {
        let ignored = [".git", "target", "node_modules", ".env", ".idea", ".vscode"];
        path.iter()
            .any(|part| ignored.contains(&part.to_string_lossy().as_ref()))
    }
}
