use asuka_core::attention::{Attention, AttentionConfig};
use asuka_core::knowledge::Document;
use clap::{command, Parser};
use rig::providers::{self, openai};

use asuka_core::character;
use asuka_core::init_logging;
use asuka_core::knowledge::KnowledgeBase;
use asuka_core::loaders::github::GitLoader;
use asuka_core::{agent::Agent, clients::discord::DiscordClient};
use sqlite_vec::sqlite3_vec_init;
use tokio_rusqlite::ffi::sqlite3_auto_extension;
use tokio_rusqlite::Connection;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to character profile TOML file
    #[arg(long, default_value = "examples/src/characters/shinobi.toml")]
    character: String,

    /// Path to database
    #[arg(long, default_value = ":memory:")]
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
    let completion_model = oai.completion_model(openai::GPT_4O);
    let should_respond_completion_model = oai.completion_model(openai::GPT_35_TURBO_0125);

    // Initialize the `sqlite-vec`extension
    // See: https://alexgarcia.xyz/sqlite-vec/rust.html
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }

    let conn = Connection::open(args.db_path).await?;
    let mut knowledge = KnowledgeBase::new(conn.clone(), embedding_model).await?;

    knowledge
        .add_documents(
            repo.with_dir("src/pages/vrf")?
                .read_with_path()
                .ignore_errors()
                .into_iter()
                .map(|(path, content)| Document {
                    id: path.to_string_lossy().to_string(),
                    source_id: "github".to_string(),
                    content,
                    created_at: chrono::Utc::now(),
                }),
        )
        .await?;

    let agent = Agent::new(character, completion_model, knowledge);

    let config = AttentionConfig {
        bot_names: vec![agent.character.name.clone()],
        ..Default::default()
    };
    let attention = Attention::new(config, should_respond_completion_model);

    let discord = DiscordClient::new(agent, attention);
    discord.start(&args.discord_api_token).await?;

    Ok(())
}
