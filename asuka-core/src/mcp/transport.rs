use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use mcp_sdk::transport::{Message, Transport};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage, WebSocketStream};

/// WebSocket transport for MCP protocol
#[derive(Clone)]
pub struct WebSocketTransport {
    ws_stream: Arc<
        Mutex<Option<WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>>,
    >,
    base_url: String,
    auth_token: Option<String>,
}

impl WebSocketTransport {
    pub fn new(base_url: &str, auth_token: Option<String>) -> Self {
        Self {
            ws_stream: Arc::new(Mutex::new(None)),
            base_url: base_url.trim_end_matches('/').to_string(),
            auth_token,
        }
    }

    async fn ensure_connected(&self) -> Result<()> {
        let mut ws_stream = self.ws_stream.lock().await;
        if ws_stream.is_none() {
            let mut request = self.base_url.as_str().into_client_request()?;

            if let Some(token) = &self.auth_token {
                request.headers_mut().insert(
                    "Authorization",
                    format!("Bearer {}", token).parse().unwrap(),
                );
            }

            let (stream, _) = connect_async(request).await?;
            *ws_stream = Some(stream);
        }
        Ok(())
    }
}

impl Transport for WebSocketTransport {
    fn send(&self, message: &Message) -> Result<()> {
        let json = serde_json::to_string(&message)?;
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            self.ensure_connected().await?;
            let mut ws_stream = self.ws_stream.lock().await;
            if let Some(stream) = ws_stream.as_mut() {
                stream.send(WsMessage::Text(json.into())).await?;
            }
            Ok(())
        })
    }

    fn receive(&self) -> Result<Message> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            self.ensure_connected().await?;
            let mut ws_stream = self.ws_stream.lock().await;
            if let Some(stream) = ws_stream.as_mut() {
                while let Some(msg) = stream.next().await {
                    let msg = msg?;
                    if let WsMessage::Text(text) = msg {
                        return Ok(serde_json::from_str(&text)?);
                    }
                }
            }
            Err(anyhow::anyhow!("WebSocket connection closed"))
        })
    }

    fn open(&self) -> Result<()> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async { self.ensure_connected().await })
    }

    fn close(&self) -> Result<()> {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut ws_stream = self.ws_stream.lock().await;
            if let Some(stream) = ws_stream.as_mut() {
                stream.close(None).await?;
            }
            Ok(())
        })
    }
}
