//! Tiny loopback WebSocket API for starting a case and waiting for its animation.

use std::{
    env,
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError},
    thread,
};

use anyhow::{Context, Result};
use case_sim::{CaseRequest, CaseResult};
use serde::Deserialize;
use serde_json::{Value, json};
use tungstenite::{Message, WebSocket, accept};

const DEFAULT_ADDR: &str = "127.0.0.1:47734";
const COMMAND_QUEUE_CAPACITY: usize = 128;

pub struct CaseApiCommand {
    pub request: CaseRequest,
    pub events: Sender<CaseApiEvent>,
}

#[derive(Clone, Debug)]
pub enum CaseApiEvent {
    Accepted,
    Completed(CaseResult),
    Cancelled,
    Rejected(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenCaseMessage {
    #[serde(rename = "type")]
    kind: String,
    request_id: Option<String>,
    viewer: Option<String>,
    tier: Option<String>,
    reward: Option<String>,
    seed: Option<u64>,
}

pub fn addr() -> String {
    env::var("SCREEN_OVERLAY_CASE_WS_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string())
}

pub fn start() -> Result<Receiver<CaseApiCommand>> {
    let address = addr();
    let socket_address: SocketAddr = address
        .parse()
        .with_context(|| format!("invalid case WebSocket address {address}"))?;
    if !socket_address.ip().is_loopback() {
        anyhow::bail!("case WebSocket address must be loopback, got {address}");
    }
    let listener = TcpListener::bind(socket_address)
        .with_context(|| format!("failed to bind case WebSocket at {address}"))?;
    start_listener(listener)
}

fn start_listener(listener: TcpListener) -> Result<Receiver<CaseApiCommand>> {
    let (commands_tx, commands_rx) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);

    thread::Builder::new()
        .name("case-websocket-listener".to_string())
        .spawn(move || accept_connections(listener, commands_tx))
        .context("failed to spawn case WebSocket listener")?;

    Ok(commands_rx)
}

fn accept_connections(listener: TcpListener, commands: SyncSender<CaseApiCommand>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let commands = commands.clone();
                if let Err(error) = thread::Builder::new()
                    .name("case-websocket-client".to_string())
                    .spawn(move || {
                        if let Err(error) = handle_client(stream, commands) {
                            eprintln!("Case WebSocket client failed: {error:#}");
                        }
                    })
                {
                    eprintln!("Could not spawn case WebSocket client thread: {error}");
                }
            }
            Err(error) => eprintln!("Case WebSocket accept failed: {error}"),
        }
    }
}

fn handle_client(stream: TcpStream, commands: SyncSender<CaseApiCommand>) -> Result<()> {
    let peer = stream
        .peer_addr()
        .context("failed to read WebSocket peer")?;
    if !is_loopback(peer.ip()) {
        anyhow::bail!("rejected non-loopback WebSocket peer {peer}");
    }
    let mut socket = accept(stream).context("case WebSocket handshake failed")?;

    loop {
        let message = match socket.read() {
            Ok(message) => message,
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(());
            }
            Err(error) => return Err(error).context("failed to read case WebSocket message"),
        };

        let text = match message {
            Message::Text(text) => text,
            Message::Close(_) => return Ok(()),
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .context("failed to answer case WebSocket ping")?;
                continue;
            }
            Message::Pong(_) => continue,
            _ => {
                send_error(&mut socket, None, "Only JSON text messages are supported.")?;
                continue;
            }
        };

        let message: OpenCaseMessage = match serde_json::from_str(&text) {
            Ok(message) => message,
            Err(error) => {
                send_error(&mut socket, None, &format!("Invalid JSON: {error}"))?;
                continue;
            }
        };
        let request_id = message.request_id.clone();
        if message.kind != "open_case" {
            send_error(
                &mut socket,
                request_id.as_deref(),
                "Unknown message type; expected open_case.",
            )?;
            continue;
        }

        let request = request_from_message(message);
        let (events_tx, events_rx) = mpsc::channel();
        match commands.try_send(CaseApiCommand {
            request,
            events: events_tx,
        }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                send_error(
                    &mut socket,
                    request_id.as_deref(),
                    "Case API is busy; try again.",
                )?;
                continue;
            }
            Err(TrySendError::Disconnected(_)) => {
                send_error(
                    &mut socket,
                    request_id.as_deref(),
                    "Case renderer is unavailable.",
                )?;
                continue;
            }
        }

        while let Ok(event) = events_rx.recv() {
            match event {
                CaseApiEvent::Accepted => {
                    send_json(
                        &mut socket,
                        json!({"type": "accepted", "requestId": request_id}),
                    )?;
                }
                CaseApiEvent::Completed(result) => {
                    send_json(
                        &mut socket,
                        json!({
                            "type": "completed",
                            "requestId": request_id,
                            "result": result_json(&result),
                        }),
                    )?;
                    break;
                }
                CaseApiEvent::Cancelled => {
                    send_json(
                        &mut socket,
                        json!({"type": "cancelled", "requestId": request_id}),
                    )?;
                    break;
                }
                CaseApiEvent::Rejected(message) => {
                    send_error(&mut socket, request_id.as_deref(), &message)?;
                    break;
                }
            }
        }
    }
}

fn request_from_message(message: OpenCaseMessage) -> CaseRequest {
    CaseRequest {
        viewer: clean_text(message.viewer)
            .filter(|viewer| !viewer.is_empty())
            .unwrap_or_else(|| "VIEWER".to_string()),
        forced_tier: clean_text(message.tier).filter(|tier| !tier.is_empty()),
        forced_reward: clean_text(message.reward).filter(|reward| !reward.is_empty()),
        seed: message.seed,
        streak: 0,
    }
}

fn clean_text(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string())
}

fn result_json(result: &CaseResult) -> Value {
    json!({
        "viewer": result.viewer,
        "tierId": result.tier_id,
        "tierName": result.tier_name,
        "rewardName": result.reward_name,
        "wheelPrize": result.wheel_prize,
    })
}

fn send_error(
    socket: &mut WebSocket<TcpStream>,
    request_id: Option<&str>,
    message: &str,
) -> Result<()> {
    send_json(
        socket,
        json!({
            "type": "error",
            "requestId": request_id,
            "message": message,
        }),
    )
}

fn send_json(socket: &mut WebSocket<TcpStream>, value: Value) -> Result<()> {
    socket
        .send(Message::Text(value.to_string().into()))
        .context("failed to write case WebSocket message")
}

fn is_loopback(ip: IpAddr) -> bool {
    ip.is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_fields_map_to_the_existing_case_request() {
        let request = request_from_message(OpenCaseMessage {
            kind: "open_case".to_string(),
            request_id: Some("job-1".to_string()),
            viewer: Some(" Alice ".to_string()),
            tier: Some(" covert ".to_string()),
            reward: Some(" Prize ".to_string()),
            seed: Some(42),
        });
        assert_eq!(request.viewer, "Alice");
        assert_eq!(request.forced_tier.as_deref(), Some("covert"));
        assert_eq!(request.forced_reward.as_deref(), Some("Prize"));
        assert_eq!(request.seed, Some(42));
    }

    #[test]
    fn blank_viewer_uses_the_public_default() {
        let request = request_from_message(OpenCaseMessage {
            kind: "open_case".to_string(),
            request_id: None,
            viewer: Some("  ".to_string()),
            tier: None,
            reward: None,
            seed: None,
        });
        assert_eq!(request.viewer, "VIEWER");
    }

    #[test]
    fn websocket_delivers_acceptance_and_completion_on_the_same_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test WebSocket");
        let address = listener.local_addr().expect("read test WebSocket address");
        let commands = start_listener(listener).expect("start test WebSocket");
        let (mut client, _) =
            tungstenite::connect(format!("ws://{address}")).expect("connect test WebSocket");

        client
            .send(Message::Text(
                json!({
                    "type": "open_case",
                    "requestId": "job-7",
                    "viewer": "Socket Viewer",
                    "seed": 7
                })
                .to_string()
                .into(),
            ))
            .expect("send open request");

        let command = commands
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("receive app command");
        assert_eq!(command.request.viewer, "Socket Viewer");
        command
            .events
            .send(CaseApiEvent::Accepted)
            .expect("send accepted event");

        let accepted = client.read().expect("read accepted event");
        let accepted: Value =
            serde_json::from_str(accepted.to_text().expect("accepted event is text"))
                .expect("accepted event is JSON");
        assert_eq!(accepted["type"], "accepted");
        assert_eq!(accepted["requestId"], "job-7");

        command
            .events
            .send(CaseApiEvent::Completed(CaseResult {
                viewer: "Socket Viewer".to_string(),
                tier_id: "covert".to_string(),
                tier_name: "Covert".to_string(),
                reward_name: "Prize".to_string(),
                wheel_prize: None,
            }))
            .expect("send completion event");

        let completed = client.read().expect("read completion event");
        let completed: Value =
            serde_json::from_str(completed.to_text().expect("completion event is text"))
                .expect("completion event is JSON");
        assert_eq!(completed["type"], "completed");
        assert_eq!(completed["requestId"], "job-7");
        assert_eq!(completed["result"]["rewardName"], "Prize");
        client.close(None).expect("close test WebSocket");
    }
}
