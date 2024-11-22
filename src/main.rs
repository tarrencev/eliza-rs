use agent::Agent;
use clap::{command, Parser};
use rig::{agent::AgentBuilder, completion::Prompt, providers};
use std::path::PathBuf;

use github::GitRepo;

mod agent;
mod character;
mod discord;
mod github;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to character profile TOML file
    #[arg(long, default_value = "src/characters/shinobi.toml")]
    character: String,

    /// Path to LanceDB database
    #[arg(long, default_value = "data/lancedb-store")]
    db_path: String,

    /// Discord API token (can also be set via DISCORD_API_TOKEN env var)
    #[arg(long, env)]
    discord_api_token: String,

    /// XAI API token (can also be set via XAI_API_KEY env var)
    #[arg(long, env = "XAI_API_KEY")]
    xai_api_key: String,

    /// GitHub repository URL
    #[arg(long, default_value = "https://github.com/cartridge-gg/docs")]
    github_repo: String,

    /// Local path to clone GitHub repository
    #[arg(long, default_value = ".repo")]
    github_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let args = Args::parse();

    let repo = GitRepo::new(args.github_repo, args.github_path);
    repo.sync()?;

    let character_content =
        std::fs::read_to_string(&args.character).expect("Failed to read character file");
    let character: character::Character =
        toml::from_str(&character_content).expect("Failed to parse character TOML");

    let db = lancedb::connect(&args.db_path).execute().await?;

    let agent = Agent::new(character, &args.xai_api_key)
        .builder()
        .context(&format!(
            "Current time: {}",
            chrono::Local::now().format("%I:%M:%S %p, %Y-%m-%d")
        ))
        .build();

    let response = agent
        .prompt("Which rust example is best suited for the operation 1 + 2")
        .await?;

    println!("{}", response);

    Ok(())
}
