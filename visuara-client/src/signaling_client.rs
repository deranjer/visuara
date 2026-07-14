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

    /// Registers a new account, falling back to logging into an existing one
    /// if that email is already taken. Shared by the controller and
    /// dashboard flows, which both just need to end up authenticated.
    pub async fn authenticate(&mut self, email: &str, password: &str) -> Result<()> {
        self.send(&ClientMessage::Register { email: email.to_string(), password: password.to_string() }).await?;
        match self.recv().await? {
            ServerMessage::AuthOk { .. } => Ok(()),
            ServerMessage::AuthError { .. } => {
                self.send(&ClientMessage::Login { email: email.to_string(), password: password.to_string() }).await?;
                match self.recv().await? {
                    ServerMessage::AuthOk { .. } => Ok(()),
                    other => anyhow::bail!("login failed: {other:?}"),
                }
            }
            other => anyhow::bail!("unexpected auth response: {other:?}"),
        }
    }
}
