use discord::Discord;
use git2::Repository;
use rig::providers;
use serenity::prelude::*;
use std::{env, path::Path};

mod character;
mod discord;

const GITHUB_PATH: &str = ".repo";
const GITHUB_REPO: &str = "https://github.com/cartridge-gg/docs";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let token =
        env::var("DISCORD_TOKEN").expect("Please set the DISCORD_TOKEN environment variable");

    std::fs::create_dir_all(GITHUB_PATH)?;

    let repo_name = GITHUB_REPO
        .split('/')
        .last()
        .unwrap_or("repo")
        .replace(".git", "");

    let clone_path = Path::new(GITHUB_PATH).join(&repo_name);
    println!("Cloning repository to {:?}", clone_path);
    Repository::clone(GITHUB_REPO, &clone_path)?;

    // Initialize LanceDB
    let db = lancedb::connect("data/lancedb-store").execute().await?;
    let model = client().completion_model(providers::xai::GROK_BETA);

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let bot = Discord::new("Shinobi".to_string());

    let mut client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Failed to create client");

    println!("Starting bot...");
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }

    Ok(())
}

fn client() -> providers::xai::Client {
    providers::xai::Client::new(&env::var("XAI_API_KEY").expect("XAI_API_KEY not set"))
}
