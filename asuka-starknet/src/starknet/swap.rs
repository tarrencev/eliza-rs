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

impl Tool for Swap {
    const NAME: &'static str = "swa[";

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
        let result = args.a + args.b;
        Ok(result)
    }
}
