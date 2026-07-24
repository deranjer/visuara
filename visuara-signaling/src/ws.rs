//! The WebSocket connection handler. Every client (whether currently acting
//! as a controller or a host — the app is symmetric) connects here, logs in,
//! and then optionally registers itself as a device (hosts do this) and/or
//! requests connections to other devices (controllers do this). SDP/ICE
//! messages are relayed between whichever two connections share a session.

use anyhow::{anyhow, bail};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use uuid::Uuid;

use visuara_common::signaling::{
    ClientMessage, ConnectCredential, DeviceId, DeviceSummary, ServerMessage,
};

use crate::state::{AppState, ConnectionId, SessionPeers};
use crate::{auth, turn};

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

enum ConnState {
    Unauthenticated,
    Authenticated { account_id: i64 },
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();
    let conn_id: ConnectionId = Uuid::new_v4();
    state.connections.insert(conn_id, tx.clone());

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if ws_tx.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut conn_state = ConnState::Unauthenticated;
    let mut my_device_id: Option<DeviceId> = None;

    while let Some(Ok(msg)) = ws_rx.next().await {
        let Message::Text(text) = msg else { continue };
        match serde_json::from_str::<ClientMessage>(&text) {
            Ok(client_msg) => {
                if let Err(e) = handle_message(
                    client_msg,
                    &state,
                    conn_id,
                    &tx,
                    &mut conn_state,
                    &mut my_device_id,
                )
                .await
                {
                    let _ = tx.send(ServerMessage::Error {
                        message: e.to_string(),
                    });
                }
            }
            Err(e) => {
                let _ = tx.send(ServerMessage::Error {
                    message: format!("malformed message: {e}"),
                });
            }
        }
    }

    state.connections.remove(&conn_id);
    if let Some(device_id) = my_device_id {
        state.device_online.remove(&device_id);
        state.otp.remove(&device_id);
    }
    send_task.abort();
}

async fn handle_message(
    msg: ClientMessage,
    state: &AppState,
    conn_id: ConnectionId,
    tx: &mpsc::UnboundedSender<ServerMessage>,
    conn_state: &mut ConnState,
    my_device_id: &mut Option<DeviceId>,
) -> anyhow::Result<()> {
    match msg {
        ClientMessage::Register { email, password } => {
            if !state.db.registration_enabled().await.unwrap_or(true) {
                tx.send(ServerMessage::AuthError {
                    message: "registration is currently disabled".into(),
                })?;
                return Ok(());
            }
            let hash = auth::hash_password(&password)?;
            match crate::session::create_account_and_maybe_bootstrap_admin(&state.db, &email, &hash).await {
                Ok((account_id, _role)) => {
                    *conn_state = ConnState::Authenticated { account_id };
                    tx.send(ServerMessage::AuthOk {
                        session_token: auth::generate_session_token(),
                    })?;
                }
                Err(e) => {
                    tx.send(ServerMessage::AuthError {
                        message: format!("registration failed: {e}"),
                    })?;
                }
            }
        }

        ClientMessage::Login { email, password } => {
            match state.db.find_account_by_email(&email).await? {
                Some(account) if auth::verify_password(&password, &account.password_hash) => {
                    *conn_state = ConnState::Authenticated {
                        account_id: account.id,
                    };
                    tx.send(ServerMessage::AuthOk {
                        session_token: auth::generate_session_token(),
                    })?;
                }
                _ => {
                    tx.send(ServerMessage::AuthError {
                        message: "invalid email or password".into(),
                    })?;
                }
            }
        }

        ClientMessage::RegisterDevice { name } => {
            let account_id = require_auth(conn_state)?;
            let device_id = match my_device_id.clone() {
                Some(id) => id,
                None => {
                    let id = auth::generate_device_id();
                    state.db.create_device(&id, account_id, &name).await?;
                    *my_device_id = Some(id.clone());
                    id
                }
            };
            state.device_online.insert(device_id.clone(), conn_id);
            let otp = auth::generate_one_time_password();
            state.otp.insert(device_id.clone(), otp.clone());
            tx.send(ServerMessage::DeviceRegistered {
                device_id,
                one_time_password: otp,
            })?;
        }

        ClientMessage::ListDevices => {
            let account_id = require_auth(conn_state)?;
            let devices = state.db.list_devices_for_account(account_id).await?;
            let summaries = devices
                .into_iter()
                .map(|d| DeviceSummary {
                    online: state.device_online.contains_key(&d.id),
                    unattended_access_enabled: d.unattended_password_hash.is_some(),
                    device_id: d.id,
                    name: d.name,
                })
                .collect();
            tx.send(ServerMessage::DeviceList { devices: summaries })?;
        }

        ClientMessage::RequestConnection {
            target_device_id,
            credential,
        } => {
            require_auth(conn_state)?;
            let device = state
                .db
                .find_device(&target_device_id)
                .await?
                .ok_or_else(|| anyhow!("unknown device"))?;

            let credential_ok = match credential {
                ConnectCredential::OneTimePassword(otp) => state
                    .otp
                    .get(&target_device_id)
                    .map(|current| *current == otp)
                    .unwrap_or(false),
                ConnectCredential::UnattendedPassword(pw) => device
                    .unattended_password_hash
                    .as_deref()
                    .map(|h| auth::verify_password(&pw, h))
                    .unwrap_or(false),
            };
            if !credential_ok {
                tx.send(ServerMessage::Error {
                    message: "invalid credential".into(),
                })?;
                return Ok(());
            }

            let host_conn_id: ConnectionId = *state
                .device_online
                .get(&target_device_id)
                .ok_or_else(|| anyhow!("device is offline"))?;

            let session_id = Uuid::new_v4().to_string();
            state
                .sessions
                .insert(session_id.clone(), SessionPeers { a: conn_id, b: host_conn_id });

            if let Some(host_tx) = state.connections.get(&host_conn_id) {
                host_tx.send(ServerMessage::IncomingConnection {
                    session_id: session_id.clone(),
                    from_device_id: None,
                })?;
            }
            tx.send(ServerMessage::ConnectionEstablished {
                session_id,
                target_device_id,
            })?;
        }

        ClientMessage::SdpOffer { session_id, sdp } => {
            relay(
                state,
                conn_id,
                &session_id,
                ServerMessage::SdpOffer { session_id: session_id.clone(), sdp },
            )?;
        }
        ClientMessage::SdpAnswer { session_id, sdp } => {
            relay(
                state,
                conn_id,
                &session_id,
                ServerMessage::SdpAnswer { session_id: session_id.clone(), sdp },
            )?;
        }
        ClientMessage::IceCandidate { session_id, candidate } => {
            relay(
                state,
                conn_id,
                &session_id,
                ServerMessage::IceCandidate { session_id: session_id.clone(), candidate },
            )?;
        }

        ClientMessage::RequestTurnCredentials => {
            let (username, password) = turn::mint_credentials(&state.turn, &conn_id.to_string(), 300);
            tx.send(ServerMessage::TurnCredentials {
                urls: state.turn.urls.clone(),
                username,
                password,
                ttl_secs: 300,
            })?;
        }

        ClientMessage::SetUnattendedPassword {
            target_device_id,
            password,
        } => {
            let account_id = require_auth(conn_state)?;
            let device = state
                .db
                .find_device(&target_device_id)
                .await?
                .ok_or_else(|| anyhow!("unknown device"))?;
            if device.account_id != account_id {
                bail!("not your device");
            }
            let hash = auth::hash_password(&password)?;
            state
                .db
                .set_unattended_password_hash(&target_device_id, &hash)
                .await?;

            if let Some(host_conn_id) = state.device_online.get(&target_device_id) {
                if let Some(host_tx) = state.connections.get(&*host_conn_id) {
                    host_tx.send(ServerMessage::UnattendedPasswordUpdated { password })?;
                }
            }
        }
    }
    Ok(())
}

fn require_auth(state: &ConnState) -> anyhow::Result<i64> {
    match state {
        ConnState::Authenticated { account_id } => Ok(*account_id),
        ConnState::Unauthenticated => bail!("not authenticated"),
    }
}

fn relay(
    state: &AppState,
    conn_id: ConnectionId,
    session_id: &str,
    msg: ServerMessage,
) -> anyhow::Result<()> {
    let peers = state
        .sessions
        .get(session_id)
        .ok_or_else(|| anyhow!("unknown session"))?;
    let other = peers.other(conn_id).ok_or_else(|| anyhow!("not part of session"))?;
    let other_tx = state
        .connections
        .get(&other)
        .ok_or_else(|| anyhow!("peer disconnected"))?;
    other_tx.send(msg)?;
    Ok(())
}
