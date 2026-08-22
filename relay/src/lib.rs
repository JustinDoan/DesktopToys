pub mod protocol;
pub mod registry;

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    validate_message_direction, CreateRoomPayload, HandshakeRequest, JoinRoomPayload, PeerRole,
    RelayEnvelope, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};
use registry::{RegistryError, RoomRegistry, PEER_QUEUE_CAPACITY};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

#[derive(Clone, Default)]
pub struct RelayState {
    registry: Arc<Mutex<RoomRegistry>>,
}

pub fn router(state: RelayState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ws", get(websocket_upgrade))
        .with_state(state)
}

pub async fn serve(listener: tokio::net::TcpListener, state: RelayState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(
            json!({ "ok": true, "service": "screen-overlay-relay", "protocolVersion": PROTOCOL_VERSION }),
        ),
    )
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<RelayState>,
) -> impl IntoResponse {
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| client_connection(socket, state))
}

struct Session {
    room_id: String,
    peer_id: String,
    authority_epoch: u64,
    role: PeerRole,
}

async fn client_connection(mut socket: WebSocket, state: RelayState) {
    let first = match tokio::time::timeout(Duration::from_secs(5), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) if text.len() <= MAX_FRAME_BYTES => text,
        _ => {
            let _ = socket
                .send(Message::Text(
                    handshake_error(
                        "invalid_handshake",
                        "The first frame must be a room handshake.",
                    )
                    .into(),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };

    let handshake: HandshakeRequest = match serde_json::from_str(&first) {
        Ok(handshake) => handshake,
        Err(_) => {
            let _ = socket
                .send(Message::Text(
                    handshake_error("invalid_handshake", "Handshake JSON was invalid.").into(),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };
    if handshake.protocol_version != PROTOCOL_VERSION {
        let _ = socket
            .send(Message::Text(
                handshake_error(
                    "unsupported_protocol",
                    "Only protocol version 1 is supported.",
                )
                .into(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    }

    let (outbound_tx, mut outbound_rx) = mpsc::channel(PEER_QUEUE_CAPACITY);
    let established = establish_session(&state, handshake, outbound_tx.clone()).await;
    let (session, welcome) = match established {
        Ok(established) => established,
        Err((code, message)) => {
            let _ = socket
                .send(Message::Text(handshake_error(code, &message).into()))
                .await;
            let _ = socket.close().await;
            return;
        }
    };

    let (mut socket_writer, mut socket_reader) = socket.split();
    if socket_writer
        .send(Message::Text(welcome.into()))
        .await
        .is_err()
    {
        cleanup_session(&state, &session).await;
        return;
    }

    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_rx.recv().await {
            if socket_writer.send(message).await.is_err() {
                break;
            }
        }
    });

    let mut last_sequence = 0;
    while let Some(frame) = socket_reader.next().await {
        match frame {
            Ok(Message::Text(text)) => {
                if text.len() > MAX_FRAME_BYTES {
                    send_session_error(
                        &outbound_tx,
                        &session,
                        "payload_too_large",
                        "Frame exceeded 1 MiB.",
                    )
                    .await;
                    break;
                }
                let envelope: RelayEnvelope = match serde_json::from_str(&text) {
                    Ok(envelope) => envelope,
                    Err(_) => {
                        send_session_error(
                            &outbound_tx,
                            &session,
                            "invalid_envelope",
                            "Envelope JSON was invalid.",
                        )
                        .await;
                        continue;
                    }
                };
                if let Err((code, message)) = validate_envelope(&session, &envelope, last_sequence)
                {
                    send_session_error(&outbound_tx, &session, code, message).await;
                    continue;
                }
                last_sequence = envelope.sequence;

                let targets = {
                    state.registry.lock().await.forward_targets(
                        &session.room_id,
                        &session.peer_id,
                        session.role,
                        envelope.target_peer_id.as_deref(),
                    )
                };
                let targets = match targets {
                    Ok(targets) => targets,
                    Err(error) => {
                        let (code, message) = registry_error(error);
                        send_session_error(&outbound_tx, &session, code, message).await;
                        continue;
                    }
                };
                for target in targets {
                    if target.try_send(Message::Text(text.clone())).is_err() {
                        send_session_error(
                            &outbound_tx,
                            &session,
                            "peer_backpressure",
                            "A destination peer could not accept the frame.",
                        )
                        .await;
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                let _ = outbound_tx.try_send(Message::Pong(payload));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    cleanup_session(&state, &session).await;
    writer.abort();
}

async fn establish_session(
    state: &RelayState,
    handshake: HandshakeRequest,
    sender: mpsc::Sender<Message>,
) -> Result<(Session, String), (&'static str, String)> {
    match handshake.message_type.as_str() {
        "relay.createRoom" | "room.hostHello" => {
            let payload: CreateRoomPayload =
                serde_json::from_value(handshake.payload).map_err(|_| {
                    (
                        "invalid_handshake",
                        "Create payload was invalid.".to_string(),
                    )
                })?;
            let nickname = validated_nickname(payload.nickname)?;
            let created = state.registry.lock().await.create_room(nickname, sender);
            let session = Session {
                room_id: created.room_id.clone(),
                peer_id: created.peer_id.clone(),
                authority_epoch: created.authority_epoch,
                role: PeerRole::Host,
            };
            let welcome = RelayEnvelope::server(
                "relay.roomCreated",
                &created.room_id,
                &created.peer_id,
                created.authority_epoch,
                json!({
                    "role": "host",
                    "inviteToken": created.invite_token,
                    "participantCount": 1
                }),
            );
            Ok((session, serde_json::to_string(&welcome).unwrap()))
        }
        "relay.joinRoom" | "room.guestHello" => {
            let payload: JoinRoomPayload = serde_json::from_value(handshake.payload)
                .map_err(|_| ("invalid_handshake", "Join payload was invalid.".to_string()))?;
            let nickname = validated_nickname(payload.nickname)?;
            let room_id = payload
                .room_id
                .or_else(|| room_id_from_invite(&payload.invite_token))
                .ok_or_else(|| {
                    (
                        "invalid_invite",
                        "Invite token did not contain a room ID.".to_string(),
                    )
                })?;
            let joined = state
                .registry
                .lock()
                .await
                .join_room(&room_id, &payload.invite_token, nickname.clone(), sender)
                .map_err(|error| {
                    let (code, message) = registry_error(error);
                    (code, message.to_string())
                })?;
            let session = Session {
                room_id: room_id.clone(),
                peer_id: joined.peer_id.clone(),
                authority_epoch: joined.authority_epoch,
                role: PeerRole::Guest,
            };
            let welcome = RelayEnvelope::server(
                "relay.roomJoined",
                &room_id,
                &joined.peer_id,
                joined.authority_epoch,
                json!({
                    "role": "guest",
                    "hostPeerId": joined.host_peer_id,
                    "participantCount": joined.participant_count
                }),
            );
            notify_host_peer_joined(state, &session, &nickname, joined.participant_count).await;
            Ok((session, serde_json::to_string(&welcome).unwrap()))
        }
        _ => Err((
            "invalid_handshake",
            "Expected room.hostHello or room.guestHello.".to_string(),
        )),
    }
}

fn room_id_from_invite(invite_token: &str) -> Option<String> {
    let (room_id, secret) = invite_token.split_once('.')?;
    if room_id.is_empty() || secret.len() < 32 {
        return None;
    }
    Some(room_id.to_string())
}

fn validated_nickname(nickname: String) -> Result<String, (&'static str, String)> {
    let nickname = nickname.trim();
    if nickname.is_empty() || nickname.chars().count() > 32 {
        return Err((
            "invalid_nickname",
            "Nickname must contain 1 to 32 characters.".to_string(),
        ));
    }
    Ok(nickname.to_string())
}

fn validate_envelope(
    session: &Session,
    envelope: &RelayEnvelope,
    last_sequence: u64,
) -> Result<(), (&'static str, &'static str)> {
    if envelope.protocol_version != PROTOCOL_VERSION {
        return Err((
            "unsupported_protocol",
            "Envelope protocol version did not match.",
        ));
    }
    if envelope.room_id != session.room_id || envelope.peer_id != session.peer_id {
        return Err((
            "identity_mismatch",
            "Envelope room or peer identity did not match the connection.",
        ));
    }
    if envelope.authority_epoch != session.authority_epoch {
        return Err((
            "stale_authority_epoch",
            "Envelope authority epoch did not match the room.",
        ));
    }
    if envelope.sequence <= last_sequence {
        return Err((
            "stale_sequence",
            "Envelope sequence was duplicate or out of order.",
        ));
    }
    if !validate_message_direction(session.role, &envelope.message_type) {
        return Err((
            "permission_denied",
            "The connection role may not send this message type.",
        ));
    }
    Ok(())
}

async fn notify_host_peer_joined(
    state: &RelayState,
    guest: &Session,
    nickname: &str,
    participant_count: usize,
) {
    let host = state
        .registry
        .lock()
        .await
        .room(&guest.room_id)
        .map(|room| room.host.sender.clone());
    let Some(host) = host else {
        return;
    };
    let event = RelayEnvelope::server(
        "room.peerJoined",
        &guest.room_id,
        &guest.peer_id,
        guest.authority_epoch,
        json!({
            "peerId": guest.peer_id,
            "nickname": nickname,
            "role": "guest",
            "participantCount": participant_count
        }),
    );
    let _ = host.try_send(Message::Text(serde_json::to_string(&event).unwrap().into()));
}

async fn cleanup_session(state: &RelayState, session: &Session) {
    let outcome =
        state
            .registry
            .lock()
            .await
            .disconnect(&session.room_id, &session.peer_id, session.role);
    let (message_type, payload) = if outcome.room_ended {
        ("room.ended", json!({ "reason": "host_disconnected" }))
    } else {
        (
            "room.peerLeft",
            json!({ "peerId": session.peer_id, "role": session.role.as_str() }),
        )
    };
    let event = RelayEnvelope::server(
        message_type,
        &session.room_id,
        &session.peer_id,
        session.authority_epoch,
        payload,
    );
    let text = Message::Text(serde_json::to_string(&event).unwrap().into());
    for target in outcome.remaining {
        let _ = target.try_send(text.clone());
    }
}

async fn send_session_error(
    sender: &mpsc::Sender<Message>,
    session: &Session,
    code: &str,
    message: &str,
) {
    let envelope = RelayEnvelope::protocol_error(
        &session.room_id,
        &session.peer_id,
        session.authority_epoch,
        code,
        message,
    );
    let _ = sender
        .send(Message::Text(
            serde_json::to_string(&envelope).unwrap().into(),
        ))
        .await;
}

fn registry_error(error: RegistryError) -> (&'static str, &'static str) {
    match error {
        RegistryError::RoomNotFound => ("room_not_found", "Room was not found."),
        RegistryError::InvalidInvite => ("invalid_invite", "Invite token was invalid."),
        RegistryError::RoomFull => ("room_full", "Room already has eight participants."),
        RegistryError::PeerNotFound => ("peer_not_found", "Peer is not a room member."),
        RegistryError::TargetNotFound => ("target_not_found", "Target peer is not a room member."),
        RegistryError::InvalidGuestTarget => {
            ("permission_denied", "Guests may only target the host.")
        }
    }
}

fn handshake_error(code: &str, message: &str) -> String {
    serde_json::to_string(&json!({
        "protocolVersion": PROTOCOL_VERSION,
        "messageType": "room.protocolError",
        "payload": { "code": code, "message": message }
    }))
    .unwrap()
}

pub fn test_envelope(
    message_type: &str,
    room_id: &str,
    peer_id: &str,
    authority_epoch: u64,
    sequence: u64,
    target_peer_id: Option<String>,
    payload: Value,
) -> RelayEnvelope {
    RelayEnvelope {
        protocol_version: PROTOCOL_VERSION,
        message_type: message_type.to_string(),
        room_id: room_id.to_string(),
        peer_id: peer_id.to_string(),
        authority_epoch,
        sequence,
        host_tick: sequence,
        sent_at_unix_ms: 1,
        target_peer_id,
        payload,
    }
}
