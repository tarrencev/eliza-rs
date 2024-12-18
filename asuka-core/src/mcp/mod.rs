use anyhow::Result;
use mcp_sdk::client::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use transport::WebSocketTransport;

mod transport;

#[derive(Clone)]
pub struct McpClient {
    inner: mcp_sdk::client::Client<WebSocketTransport>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpEndpoint {
    pub url: String,
    pub auth_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON schema for the tool's parameters
}

impl McpClient {
    pub async fn new(endpoint: McpEndpoint) -> Result<Self> {
        let transport = WebSocketTransport::new(&endpoint.url, endpoint.auth_token);
        let client = Client::builder(transport).build();

        // Initialize the client
        client
            .initialize(mcp_sdk::types::Implementation {
                name: "asuka".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            })
            .await?;

        Ok(Self { inner: client })
    }

    pub async fn get_tools(&self) -> Result<Vec<ToolDefinition>> {
        let response = self
            .inner
            .request(
                "tools/list",
                None,
                mcp_sdk::protocol::RequestOptions::default(),
            )
            .await?;

        let tools: Vec<ToolDefinition> = serde_json::from_value(response)?;
        Ok(tools)
    }

    pub async fn execute_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.inner
            .request(
                &format!("tools/{}/execute", name),
                Some(args),
                mcp_sdk::protocol::RequestOptions::default(),
            )
            .await
    }
}
