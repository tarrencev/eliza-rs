use rig::{completion::ToolDefinition, tool::Tool};
use serde::Deserialize;
use serde_json::json;
use starknet::core::types::Felt;
use tokio_rusqlite::Connection;

pub const INIT_SQL: &str = "
BEGIN;
-- Account table
CREATE TABLE IF NOT EXISTS accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT UNIQUE NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_account_address ON accounts(address);

-- Token table  
CREATE TABLE IF NOT EXISTS tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    address TEXT UNIQUE NOT NULL,
    name TEXT NOT NULL,
    symbol TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_token_address ON tokens(address);
CREATE INDEX IF NOT EXISTS idx_token_name ON tokens(name);
CREATE INDEX IF NOT EXISTS idx_token_symbol ON tokens(symbol);
COMMIT;";

#[derive(Deserialize)]
pub struct TransferArgs {
    recipient: String,
    amount: Felt,
    token: String, // Changed to String to accept name/symbol
}

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Token not found")]
    TokenNotFound,
    #[error("Invalid recipient address")]
    InvalidRecipient,
    #[error("Database error: {0}")]
    DatabaseError(#[from] tokio_rusqlite::Error),
}

pub struct Transfer {
    conn: Connection,
}

impl Transfer {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    async fn lookup_token(&self, token: &str) -> Result<Felt, TransferError> {
        let token = token.to_lowercase();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT address FROM tokens WHERE LOWER(name) = ? OR LOWER(symbol) = ?",
                )?;
                let mut rows = stmt.query([&token, &token])?;

                if let Some(row) = rows.next()? {
                    let address: String = row.get(0)?;
                    Ok(Some(address))
                } else {
                    Ok(None)
                }
            })
            .await?;

        match result {
            Some(address) => {
                Ok(Felt::from_hex(&address).map_err(|_| TransferError::TokenNotFound)?)
            }
            None => Err(TransferError::TokenNotFound),
        }
    }

    async fn lookup_recipient(&self, recipient: &str) -> Result<Felt, TransferError> {
        // First try parsing as hex
        if let Ok(address) = Felt::from_hex(recipient) {
            return Ok(address);
        }

        // Otherwise look up in accounts table
        let recipient = recipient.to_lowercase();
        let result = self
            .conn
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT address FROM accounts WHERE LOWER(name) = ?")?;
                let mut rows = stmt.query([recipient])?;

                if let Some(row) = rows.next()? {
                    let address: String = row.get(0)?;
                    Ok(Some(address))
                } else {
                    Ok(None)
                }
            })
            .await?;

        match result {
            Some(address) => {
                Ok(Felt::from_hex(&address).map_err(|_| TransferError::InvalidRecipient)?)
            }
            None => Err(TransferError::InvalidRecipient),
        }
    }
}

impl Tool for Transfer {
    const NAME: &'static str = "transfer";

    type Error = TransferError;
    type Args = TransferArgs;
    type Output = Felt;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "transfer".to_string(),
            description: "Transfer tokens to a recipient".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "recipient": {
                        "type": "string",
                        "description": "The recipient address or account name"
                    },
                    "amount": {
                        "type": "string",
                        "description": "The amount to transfer"
                    },
                    "token": {
                        "type": "string",
                        "description": "The token name, symbol or contract address"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let token_address = self.lookup_token(&args.token).await?;
        let recipient_address = self.lookup_recipient(&args.recipient).await?;

        // Here we would implement the actual transfer logic
        // For now just return a dummy transaction hash
        Ok(Felt::ZERO)
    }
}
