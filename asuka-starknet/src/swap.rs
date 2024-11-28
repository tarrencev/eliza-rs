use rig::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use starknet::core::types::Felt;

#[derive(Deserialize)]
pub struct SwapArgs {
    a: Felt,
    b: Felt,
}

#[derive(Debug, thiserror::Error)]
#[error("Swap error")]
pub struct SwapError;

#[derive(Deserialize, Serialize)]
pub struct Swap;

#[derive(Deserialize)]
struct PoolKey {
    token0: String,
    token1: String,
    fee: String,
    tick_spacing: i32,
    extension: String,
}

#[derive(Deserialize)]
struct Route {
    pool_key: PoolKey,
    sqrt_ratio_limit: String,
    skip_ahead: i32,
}

#[derive(Deserialize)]
struct Split {
    amount: String,
    specified_amount: String,
    route: Vec<Route>,
}

#[derive(Deserialize)]
struct QuoteResponse {
    total: String,
    splits: Vec<Split>,
}

impl Tool for Swap {
    const NAME: &'static str = "swap";

    type Error = SwapError;
    type Args = SwapArgs;
    type Output = Felt;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "swap".to_string(),
            description: "Swap token a for token b".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": {
                        "type": "string",
                        "description": "The token to buy"
                    },
                    "b": {
                        "type": "string",
                        "description": "The token to sell"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let url = format!(
            "https://mainnet-api.ekubo.org/quote/{}/{}/{}",
            "-1e9", // Hardcoded amount for example
            args.a.to_string(),
            args.b.to_string()
        );

        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("accept", "application/json")
            .send()
            .await
            .map_err(|_| SwapError)?
            .json::<QuoteResponse>()
            .await
            .map_err(|_| SwapError)?;

        let total = Felt::from_hex(&response.total).map_err(|_| SwapError)?;

        Ok(total)
    }
}
