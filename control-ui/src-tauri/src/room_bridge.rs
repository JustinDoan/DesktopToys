//! Persistent companion for the native localhost room bridge.
//!
//! The legacy command socket remains the compatibility path while this bridge
//! carries room lifecycle and high-rate scene frames without opening a TCP
//! connection per update.

use std::{
    env,
    io::{Read, Write},
    net::TcpStream,
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_BRIDGE_ADDR: &str = "127.0.0.1:47733";
const VERSION: u8 = 1;
const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeMessage {
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
}

pub struct RoomBridge {
    sender: Option<SyncSender<BridgeMessage>>,
    incoming: Arc<Mutex<Receiver<BridgeMessage>>>,
}

impl Clone for RoomBridge {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            incoming: self.incoming.clone(),
        }
    }
}

impl Default for RoomBridge {
    fn default() -> Self {
        let (_sender, incoming) = mpsc::sync_channel(1);
        Self {
            sender: None,
            incoming: Arc::new(Mutex::new(incoming)),
        }
    }
}

impl RoomBridge {
    pub fn start_from_environment() -> Self {
        let Ok(token) = env::var("SCREEN_OVERLAY_ROOM_BRIDGE_TOKEN") else {
            return Self::default();
        };
        if token.len() != 64 {
            return Self::default();
        }
        let (sender, receiver) = mpsc::sync_channel(256);
        let (incoming_tx, incoming) = mpsc::sync_channel(256);
        thread::Builder::new()
            .name("room-native-bridge".to_string())
            .spawn(move || run(token, receiver, incoming_tx))
            .ok();
        Self {
            sender: Some(sender),
            incoming: Arc::new(Mutex::new(incoming)),
        }
    }

    pub fn send(&self, kind: &str, payload: Value) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(BridgeMessage {
                kind: kind.to_string(),
                payload,
            });
        }
    }
    pub fn drain(&self) -> Vec<BridgeMessage> {
        let Ok(receiver) = self.incoming.lock() else {
            return Vec::new();
        };
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }
}

fn run(
    token: String,
    receiver: mpsc::Receiver<BridgeMessage>,
    incoming: SyncSender<BridgeMessage>,
) {
    let address = env::var("SCREEN_OVERLAY_ROOM_BRIDGE_ADDR")
        .unwrap_or_else(|_| DEFAULT_BRIDGE_ADDR.to_string());
    let Ok(mut stream) = TcpStream::connect(address) else {
        return;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(80)));
    if write_frame(
        &mut stream,
        1,
        &BridgeMessage {
            kind: "hello".to_string(),
            payload: serde_json::json!({ "token": token }),
        },
    )
    .is_err()
    {
        return;
    }
    let Ok(Some((_, ready))) = read_frame(&mut stream) else {
        return;
    };
    if ready.kind != "ready" {
        return;
    }
    crate::room_trace("bridge authenticated to native");
    loop {
        while let Ok(message) = receiver.try_recv() {
            if write_frame(&mut stream, 2, &message).is_err() {
                return;
            }
            let relay_type = message.payload.get("messageType").and_then(Value::as_str);
            if message.kind != "relayFrame"
                || !matches!(
                    relay_type,
                    Some("scene.transformBatch" | "interaction.moveGrab")
                )
            {
                crate::room_trace(format!("bridge ui->native kind={}", message.kind));
            }
        }
        match read_frame(&mut stream) {
            Ok(Some((_kind, message))) => {
                if !matches!(
                    message.kind.as_str(),
                    "scene.transformBatch" | "interaction.moveGrab"
                ) {
                    crate::room_trace(format!("bridge native->ui kind={}", message.kind));
                }
                let _ = incoming.try_send(message);
            }
            Ok(None) => return,
            Err(_) => {}
        }
    }
}

fn read_frame(stream: &mut TcpStream) -> Result<Option<(u8, BridgeMessage)>, std::io::Error> {
    let mut length = [0_u8; 4];
    match read_frame_part(stream, &mut length, true)? {
        FramePartRead::Complete => {}
        FramePartRead::Idle => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "idle read",
            ))
        }
        FramePartRead::Eof => return Ok(None),
    }
    let length = u32::from_le_bytes(length) as usize;
    if !(2..=MAX_FRAME_BYTES).contains(&length) {
        return Ok(None);
    }
    let mut frame = vec![0_u8; length];
    if read_frame_part(stream, &mut frame, false)? == FramePartRead::Eof {
        return Ok(None);
    }
    if frame[0] != VERSION {
        return Ok(None);
    }
    let message = serde_json::from_slice(&frame[2..]).map_err(std::io::Error::other)?;
    Ok(Some((frame[1], message)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FramePartRead {
    Complete,
    Idle,
    Eof,
}

fn read_frame_part(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    idle_before_frame: bool,
) -> Result<FramePartRead, std::io::Error> {
    let mut offset = 0;
    while offset < buffer.len() {
        match stream.read(&mut buffer[offset..]) {
            Ok(0) => return Ok(FramePartRead::Eof),
            Ok(read) => offset += read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if offset == 0 && idle_before_frame {
                    return Ok(FramePartRead::Idle);
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(FramePartRead::Complete)
}

fn write_frame(
    stream: &mut TcpStream,
    kind: u8,
    message: &BridgeMessage,
) -> Result<(), std::io::Error> {
    let payload = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    if payload.len() + 2 > MAX_FRAME_BYTES {
        return Err(std::io::Error::other("bridge frame too large"));
    }
    stream.write_all(&((payload.len() + 2) as u32).to_le_bytes())?;
    stream.write_all(&[VERSION, kind])?;
    stream.write_all(&payload)?;
    stream.flush()
}
