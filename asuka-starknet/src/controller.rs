use once_cell::sync::Lazy;
use rig::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slot::session::PolicyMethod;
use starknet::core::types::Felt;
use url::Url;

/// Flow:
/// 1. User messages Agent to create Controller Session with policies
/// 2. Agent creates URL for controller session creation
/// 3. Agent replies with session creation URL
/// 4. User clicks link and authorizes session
///
/// Example:
/// ```
/// User: "Create a controller session that can only swap tokens"
/// Agent: Creates URL with swap policy
/// Agent: "Click here to authorize the session: https://..."
/// User: Clicks link and approves session in wallet
/// ```

static RPC_URL: Lazy<Url> =
    Lazy::new(|| Url::parse("https://api.cartridge.gg/x/starknet/mainnet").unwrap());

#[derive(Deserialize)]
pub struct ControllerArgs {
    policies: Vec<PolicyMethod>,
}

#[derive(Debug, thiserror::Error)]
pub enum ControllerError {
    #[error(transparent)]
    Slot(#[from] slot::Error),
    // Add other error variants as needed
}

#[derive(Deserialize, Serialize)]
pub struct Controller;

impl Tool for Controller {
    const NAME: &'static str = "controller";

    type Error = ControllerError;
    type Args = ControllerArgs;
    type Output = Felt;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "controller".to_string(),
            description: "Create a new Cartridge Controller account based on session key"
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "contracts": {
                        "type": "object",
                        "description": "Map of contract info"
                    }
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let session = slot::session::create(RPC_URL.clone(), &args.policies).await?;

        return Ok(Felt::ZERO);
    }
}
