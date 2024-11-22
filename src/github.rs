use anyhow::Result;
use git2::{FetchOptions, RemoteCallbacks, Repository};
use std::path::PathBuf;

pub struct GitRepo {
    url: String,
    path: PathBuf,
}

impl GitRepo {
    pub fn new(url: String, path: PathBuf) -> Self {
        Self { url, path }
    }

    pub fn sync(&self) -> Result<Repository> {
        if self.path.exists() {
            self.reset()
        } else {
            self.clone()
        }
    }

    fn clone(&self) -> Result<Repository> {
        std::fs::create_dir_all(&self.path)?;
        Ok(Repository::clone(
            &self.url,
            &self.path.join(self.repo_name()),
        )?)
    }

    fn reset(&self) -> Result<Repository> {
        let repo = Repository::open(&self.path)?;

        {
            let mut remote = repo.find_remote("origin")?;
            let callbacks = RemoteCallbacks::new();
            let mut fetch_options = FetchOptions::new();
            fetch_options.remote_callbacks(callbacks);
            remote.fetch(&["main"], Some(&mut fetch_options), None)?;

            let main_ref = repo.find_reference("refs/remotes/origin/main")?;
            let main_commit = main_ref.peel_to_commit()?;

            let mut checkout_builder = git2::build::CheckoutBuilder::new();
            repo.reset(
                &main_commit.as_object(),
                git2::ResetType::Hard,
                Some(&mut checkout_builder),
            )?;
        }

        Ok(repo)
    }

    pub fn repo_name(&self) -> String {
        self.url
            .split('/')
            .last()
            .unwrap_or("repo")
            .replace(".git", "")
            .to_string()
    }
}
