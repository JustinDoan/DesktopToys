//! Blocking localhost relay client used before Railway is configured.
//!
//! It speaks the same raw WebSocket handshake/envelope shape as the planned
//! hosted client. The session owns one socket on a worker thread so dropping
//! the room service is enough to close the local membership.

use std::{
    env,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    thread,
    time::Duration,
};

use serde_json::{json, Value};
use tungstenite::{connect, stream::MaybeTlsStream, Message, WebSocket};

const DEFAULT_RELAY_URL: &str = "ws://127.0.0.1:8787/ws";

pub struct RelaySession {
    pub outgoing: SyncSender<Value>,
    pub incoming: Receiver<Value>,
}

pub struct RelayWelcome {
    pub room_id: String,
    pub peer_id: String,
    pub authority_epoch: u64,
    pub invite_token: Option<String>,
    pub host_peer_id: Option<String>,
    pub session: RelaySession,
}

pub fn connect_host(nickname: &str) -> Result<RelayWelcome, String> {
    connect_host_at(&relay_url(), nickname)
}

pub fn connect_guest(
    room_id: &str,
    invite_token: &str,
    nickname: &str,
) -> Result<RelayWelcome, String> {
    connect_guest_at(&relay_url(), room_id, invite_token, nickname)
}

fn connect_host_at(url: &str, nickname: &str) -> Result<RelayWelcome, String> {
    connect_with_handshake(
        url,
        json!({ "protocolVersion": 1, "messageType": "room.hostHello", "payload": { "nickname": nickname, "hostSessionId": client_id() } }),
    )
}
fn connect_guest_at(
    url: &str,
    room_id: &str,
    invite_token: &str,
    nickname: &str,
) -> Result<RelayWelcome, String> {
    connect_with_handshake(
        url,
        json!({ "protocolVersion": 1, "messageType": "room.guestHello", "payload": { "roomId": room_id, "inviteToken": invite_token, "nickname": nickname, "clientInstanceId": client_id() } }),
    )
}
fn relay_url() -> String {
    env::var("SCREEN_OVERLAY_LOCAL_RELAY_URL").unwrap_or_else(|_| DEFAULT_RELAY_URL.to_string())
}

fn connect_with_handshake(url: &str, handshake: Value) -> Result<RelayWelcome, String> {
    let (mut socket, _) =
        connect(url).map_err(|error| format!("local relay connect failed: {error}"))?;
    socket
        .send(Message::Text(handshake.to_string().into()))
        .map_err(|error| format!("local relay handshake failed: {error}"))?;
    let welcome = read_json(&mut socket)?;
    if welcome.get("messageType").and_then(Value::as_str) == Some("room.protocolError") {
        return Err(welcome
            .pointer("/payload/message")
            .and_then(Value::as_str)
            .unwrap_or("relay rejected room request")
            .to_string());
    }
    let room_id = welcome
        .get("roomId")
        .and_then(Value::as_str)
        .ok_or_else(|| "relay welcome omitted room ID".to_string())?
        .to_string();
    let peer_id = welcome
        .get("peerId")
        .and_then(Value::as_str)
        .ok_or_else(|| "relay welcome omitted peer ID".to_string())?
        .to_string();
    let authority_epoch = welcome
        .get("authorityEpoch")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let invite_token = welcome
        .pointer("/payload/inviteToken")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let host_peer_id = welcome
        .pointer("/payload/hostPeerId")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let (outgoing, outgoing_rx) = mpsc::sync_channel(256);
    let (incoming_tx, incoming) = mpsc::sync_channel(256);
    thread::Builder::new()
        .name("local-room-relay".to_string())
        .spawn(move || relay_loop(socket, outgoing_rx, incoming_tx))
        .map_err(|error| error.to_string())?;
    Ok(RelayWelcome {
        room_id,
        peer_id,
        authority_epoch,
        invite_token,
        host_peer_id,
        session: RelaySession { outgoing, incoming },
    })
}

fn relay_loop(
    mut socket: WebSocket<MaybeTlsStream<std::net::TcpStream>>,
    outgoing: Receiver<Value>,
    incoming: SyncSender<Value>,
) {
    if let MaybeTlsStream::Plain(stream) = socket.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(25)));
    }
    loop {
        match outgoing.try_recv() {
            Ok(frame)
                if socket
                    .send(Message::Text(frame.to_string().into()))
                    .is_err() =>
            {
                break
            }
            Ok(_) | Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Ok(frame) = serde_json::from_str::<Value>(&text) {
                    let _ = incoming.try_send(frame);
                }
            }
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed)
            | Err(tungstenite::Error::AlreadyClosed) => break,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
            _ => {}
        }
    }
}

fn read_json(socket: &mut WebSocket<MaybeTlsStream<std::net::TcpStream>>) -> Result<Value, String> {
    loop {
        match socket
            .read()
            .map_err(|error| format!("local relay welcome failed: {error}"))?
        {
            Message::Text(text) => {
                return serde_json::from_str(&text)
                    .map_err(|error| format!("invalid relay welcome: {error}"))
            }
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .map_err(|error| error.to_string())?,
            Message::Close(_) => return Err("local relay closed during handshake".to_string()),
            _ => {}
        }
    }
}

fn client_id() -> String {
    format!("local-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn host_frame_reaches_guest_through_the_real_loopback_relay() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let address = listener.local_addr().unwrap();
        let task = runtime.spawn(screen_overlay_relay::serve(
            listener,
            screen_overlay_relay::RelayState::default(),
        ));
        let url = format!("ws://{address}/ws");
        let mut host = connect_host_at(&url, "Host").unwrap();
        let invite = host.invite_token.take().unwrap();
        let guest = connect_guest_at(&url, &host.room_id, &invite, "Guest").unwrap();
        host.session.outgoing.send(json!({ "protocolVersion": 1, "messageType": "scene.transformBatch", "roomId": host.room_id, "peerId": host.peer_id, "authorityEpoch": host.authority_epoch, "sequence": 1, "hostTick": 1, "sentAtUnixMs": 1, "targetPeerId": null, "payload": { "hostTick": 1, "transforms": [] } })).unwrap();
        let received = guest
            .session
            .incoming
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            received.get("messageType").and_then(Value::as_str),
            Some("scene.transformBatch")
        );
        guest.session.outgoing.send(json!({ "protocolVersion": 1, "messageType": "interaction.beginGrab", "roomId": guest.room_id, "peerId": guest.peer_id, "authorityEpoch": guest.authority_epoch, "sequence": 1, "hostTick": 1, "sentAtUnixMs": 2, "targetPeerId": host.peer_id, "payload": { "intentId": "grab-1", "networkEntityId": "1:42", "pointer": { "x": 10.0, "y": 20.0 }, "observedHostTick": 1, "inputSequence": 1, "clientMonotonicMs": 2 } })).unwrap();
        let host_intent = loop {
            let frame = host
                .session
                .incoming
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            if frame.get("messageType").and_then(Value::as_str) == Some("interaction.beginGrab") {
                break frame;
            }
        };
        assert_eq!(
            host_intent
                .pointer("/payload/networkEntityId")
                .and_then(Value::as_str),
            Some("1:42")
        );
        task.abort();
    }
}
