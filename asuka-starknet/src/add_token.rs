use rig::{completion::ToolDefinition, tool::Tool};
use serde::Deserialize;
use serde_json::json;
use starknet::core::types::Felt;
use tokio_rusqlite::Connection;

#[derive(Deserialize)]
pub struct AddTokenArgs {
    name: String,
    symbol: String,
    address: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AddTokenError {
    #[error("Invalid token address")]
    InvalidAddress,
    #[error("Database error: {0}")]
    DatabaseError(#[from] tokio_rusqlite::Error),
}

pub struct AddToken {
    conn: Connection,
}

impl AddToken {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }
}

impl Tool for AddToken {
    const NAME: &'static str = "add_token";

    type Error = AddTokenError;
    type Args = AddTokenArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "add_token".to_string(),
            description: "Add a new token".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The name of the token"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "The symbol of the token"
                    },
                    "address": {
                        "type": "string",
                        "description": "The contract address of the token"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        // Validate the address is a valid Felt
        Felt::from_hex(&args.address).map_err(|_| AddTokenError::InvalidAddress)?;
        let (name, symbol, address) =
            (args.name.clone(), args.symbol.clone(), args.address.clone());

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO tokens (name, symbol, address) VALUES (?1, ?2, ?3)",
                    [&name, &symbol, &address],
                )
                .map_err(tokio_rusqlite::Error::from)
            })
            .await?;

        Ok(format!(
            "Added token {} ({}) at address {}",
            args.name, args.symbol, args.address
        ))
    }
}
