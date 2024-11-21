use discord::Discord;
use git2::Repository;
use rig::{agent::AgentBuilder, providers};
use serenity::prelude::*;
use std::{env, path::Path};

mod character;
mod discord;

const GITHUB_PATH: &str = ".repo";
const GITHUB_REPO: &str = "https://github.com/cartridge-gg/docs";
const CHARACTER: &str = "../characters/shinobi.toml";

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

    let character_content =
        std::fs::read_to_string(CHARACTER).expect("Failed to read character file");
    let character: character::Character =
        toml::from_str(&character_content).expect("Failed to parse character TOML");

    // Initialize LanceDB
    let db = lancedb::connect("data/lancedb-store").execute().await?;
    let model = client().completion_model(providers::xai::GROK_BETA);

    let agent = AgentBuilder::new(model)
        .context("Definition of a *flurbo*: A flurbo is a green alien that lives on cold planets")
        .context("Definition of a *glarb-glarb*: A glarb-glarb is a ancient tool used by the ancestors of the inhabitants of planet Jiro to farm the land.")
        .context("Definition of a *linglingdong*: A term used by inhabitants of the far side of the moon to describe humans.")
        .build();

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
