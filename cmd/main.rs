use clap::{command, Parser};
use rig::{
    completion::Prompt,
    providers::{self, openai},
    vector_store::in_memory_store::InMemoryVectorStore,
};

use asuka::agent::Agent;
use asuka::character;
use asuka::init_logging;
use asuka::knowledge::KnowledgeBase;
use asuka::loaders::github::GitLoader;

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

    /// OpenAI API token (can also be set via OPENAI_API_KEY env var)
    #[arg(long, env = "OPENAI_API_KEY")]
    openai_api_key: String,

    /// GitHub repository URL
    #[arg(long, default_value = "https://github.com/cartridge-gg/docs")]
    github_repo: String,

    /// Local path to clone GitHub repository
    #[arg(long, default_value = ".repo")]
    github_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    dotenv::dotenv().ok();

    let args = Args::parse();

    let repo = GitLoader::new(args.github_repo, &args.github_path)?;

    let character_content =
        std::fs::read_to_string(&args.character).expect("Failed to read character file");
    let character: character::Character =
        toml::from_str(&character_content).expect("Failed to parse character TOML");

    let oai = providers::openai::Client::new(&args.openai_api_key);
    let embedding_model = oai.embedding_model(openai::TEXT_EMBEDDING_3_SMALL);

    let xai = providers::xai::Client::new(&args.xai_api_key);
    let completion_model = xai.completion_model(providers::xai::GROK_BETA);

    let store = InMemoryVectorStore::default();
    let mut knowledge = KnowledgeBase::new(store, embedding_model);

    // Add some example documents
    knowledge
        .add_documents(
            repo.with_dir("src/pages/vrf")?
                .read_with_path()
                .ignore_errors(),
        )
        .await?;

    let agent = Agent::new(character, completion_model)
        .with_knowledge(&knowledge)
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
