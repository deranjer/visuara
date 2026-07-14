//! Thin WebSocket client for talking to the signaling server: send
//! `ClientMessage`s, receive `ServerMessage`s.

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use visuara_common::signaling::{ClientMessage, ServerMessage};

pub struct SignalingClient {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl SignalingClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .with_context(|| format!("connect to signaling server at {url}"))?;
        Ok(Self { ws })
    }

    pub async fn send(&mut self, msg: &ClientMessage) -> Result<()> {
        let text = serde_json::to_string(msg).context("serialize client message")?;
        self.ws
            .send(Message::Text(text.into()))
            .await
            .context("send signaling message")
    }

    pub async fn recv(&mut self) -> Result<ServerMessage> {
        loop {
            let msg = self
                .ws
                .next()
                .await
                .context("signaling connection closed")?
                .context("websocket error")?;
            if let Message::Text(text) = msg {
                return serde_json::from_str(&text).context("parse server message");
            }
        }
    }
}
