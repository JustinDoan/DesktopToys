//! Authenticated, length-prefixed localhost bridge used by the native overlay
//! and the Tauri room service. It is deliberately transport-only: room and
//! scene policy remain in their respective runtimes.

use std::{
    env,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_BRIDGE_ADDR: &str = "127.0.0.1:47733";
const BRIDGE_VERSION: u8 = 1;
const HANDSHAKE_KIND: u8 = 1;
const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeMessage {
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
}

pub struct NativeBridge {
    pub token: String,
    pub incoming: Receiver<BridgeMessage>,
    pub outgoing: SyncSender<BridgeMessage>,
}

pub fn start() -> Result<NativeBridge> {
    let address = bridge_addr();
    let listener =
        TcpListener::bind(&address).with_context(|| format!("failed to bind {address}"))?;
    let token = random_token()?;
    let (incoming_tx, incoming) = mpsc::sync_channel(256);
    let (outgoing, outgoing_rx) = mpsc::sync_channel(256);
    let token_for_thread = token.clone();
    thread::Builder::new()
        .name("room-local-bridge".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Err(error) =
                            serve_client(stream, &token_for_thread, &incoming_tx, &outgoing_rx)
                        {
                            crate::room_trace(format!("bridge client ended: {error:#}"));
                        }
                    }
                    Err(error) => eprintln!("Room bridge accept failed: {error}"),
                }
            }
        })
        .context("failed to start room bridge")?;
    Ok(NativeBridge {
        token,
        incoming,
        outgoing,
    })
}

pub fn bridge_addr() -> String {
    env::var("SCREEN_OVERLAY_ROOM_BRIDGE_ADDR").unwrap_or_else(|_| DEFAULT_BRIDGE_ADDR.to_string())
}

fn serve_client(
    mut stream: TcpStream,
    token: &str,
    incoming: &SyncSender<BridgeMessage>,
    outgoing: &Receiver<BridgeMessage>,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(80)))?;
    let handshake = read_frame(&mut stream)?.context("bridge closed before handshake")?;
    if handshake.0 != HANDSHAKE_KIND
        || handshake.1.kind != "hello"
        || handshake.1.payload.get("token").and_then(Value::as_str) != Some(token)
    {
        return Ok(());
    }
    write_frame(
        &mut stream,
        HANDSHAKE_KIND,
        &BridgeMessage {
            kind: "ready".to_string(),
            payload: Value::Null,
        },
    )?;
    crate::room_trace("bridge client authenticated");
    loop {
        while let Ok(message) = outgoing.try_recv() {
            if !matches!(
                message.kind.as_str(),
                "scene.transformBatch" | "interaction.moveGrab"
            ) {
                crate::room_trace(format!("bridge native->ui kind={}", message.kind));
            }
            write_frame(&mut stream, 2, &message)?;
        }
        match read_frame(&mut stream) {
            Ok(Some((_, message))) => {
                let relay_type = message.payload.get("messageType").and_then(Value::as_str);
                if message.kind != "relayFrame"
                    || !matches!(
                        relay_type,
                        Some("scene.transformBatch" | "interaction.moveGrab")
                    )
                {
                    crate::room_trace(format!("bridge ui->native kind={}", message.kind));
                }
                let _ = incoming.try_send(message);
            }
            Ok(None) => return Ok(()),
            Err(error) if is_idle_read_timeout(&error) => {}
            Err(error) => return Err(error),
        }
    }
}

fn is_idle_read_timeout(error: &anyhow::Error) -> bool {
    error
        .root_cause()
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| {
            matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
        })
}

fn read_frame(stream: &mut TcpStream) -> Result<Option<(u8, BridgeMessage)>> {
    let mut length = [0_u8; 4];
    match read_frame_part(stream, &mut length, true)? {
        FramePartRead::Complete => {}
        FramePartRead::Idle => {
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "idle read").into());
        }
        FramePartRead::Eof => return Ok(None),
    }
    let length = u32::from_le_bytes(length) as usize;
    if !(2..=MAX_FRAME_BYTES).contains(&length) {
        anyhow::bail!("invalid bridge frame length");
    }
    let mut frame = vec![0_u8; length];
    if read_frame_part(stream, &mut frame, false)? == FramePartRead::Eof {
        return Ok(None);
    }
    if frame[0] != BRIDGE_VERSION {
        anyhow::bail!("unsupported bridge version");
    }
    Ok(Some((
        frame[1],
        serde_json::from_slice(&frame[2..]).context("invalid bridge JSON")?,
    )))
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
) -> std::io::Result<FramePartRead> {
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

fn write_frame(stream: &mut TcpStream, kind: u8, message: &BridgeMessage) -> Result<()> {
    let payload = serde_json::to_vec(message)?;
    let length = 2 + payload.len();
    if length > MAX_FRAME_BYTES {
        anyhow::bail!("bridge frame too large");
    }
    stream.write_all(&(length as u32).to_le_bytes())?;
    stream.write_all(&[BRIDGE_VERSION, kind])?;
    stream.write_all(&payload)?;
    stream.flush()?;
    Ok(())
}

fn random_token() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("failed to generate bridge token: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn frame_limits_and_token_shape_are_enforced() {
        assert_eq!(random_token().unwrap().len(), 64);
        assert!(MAX_FRAME_BYTES >= 1024 * 1024);
    }

    #[test]
    fn authenticated_client_can_send_a_framed_room_message() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (incoming_tx, incoming_rx) = mpsc::sync_channel(1);
        let (_outgoing_tx, outgoing_rx) = mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            serve_client(stream, "test-token", &incoming_tx, &outgoing_rx).unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        write_frame(
            &mut client,
            HANDSHAKE_KIND,
            &BridgeMessage {
                kind: "hello".to_string(),
                payload: serde_json::json!({ "token": "test-token" }),
            },
        )
        .unwrap();
        assert_eq!(read_frame(&mut client).unwrap().unwrap().1.kind, "ready");
        // Windows commonly reports an idle read timeout as WouldBlock (10035).
        // The persistent bridge must remain alive between room messages.
        thread::sleep(Duration::from_millis(180));
        let message = BridgeMessage {
            kind: "enterHost".to_string(),
            payload: Value::Null,
        };
        let payload = serde_json::to_vec(&message).unwrap();
        let length = (payload.len() + 2) as u32;
        client.write_all(&length.to_le_bytes()[..2]).unwrap();
        thread::sleep(Duration::from_millis(180));
        client.write_all(&length.to_le_bytes()[2..]).unwrap();
        client.write_all(&[BRIDGE_VERSION, 2]).unwrap();
        client.write_all(&payload).unwrap();
        client.flush().unwrap();
        assert_eq!(
            incoming_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .kind,
            "enterHost"
        );
        drop(client);
        server.join().unwrap();
    }
}
