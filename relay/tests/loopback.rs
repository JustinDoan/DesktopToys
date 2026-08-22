use futures_util::{SinkExt, StreamExt};
use screen_overlay_relay::{serve, test_envelope, RelayState};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn spawn_relay() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        serve(listener, RelayState::default()).await.unwrap();
    });
    (format!("127.0.0.1:{}", address.port()), task)
}

async fn next_json<S>(socket: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = socket.next().await.unwrap().unwrap();
    let Message::Text(text) = message else {
        panic!("expected text message");
    };
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn health_endpoint_reports_protocol_version() {
    let (address, relay) = spawn_relay().await;
    let health: Value = reqwest::get(format!("http://{address}/health"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["protocolVersion"], 1);
    relay.abort();
}

#[tokio::test]
async fn host_and_guest_forward_frames_in_both_allowed_directions() {
    let (address, relay) = spawn_relay().await;
    let (mut host, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    host.send(Message::Text(
        json!({
            "protocolVersion": 1,
            "messageType": "room.hostHello",
            "payload": { "nickname": "Host", "hostSessionId": "host-session-1" }
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();
    let created = next_json(&mut host).await;
    assert_eq!(created["messageType"], "relay.roomCreated");
    let room_id = created["roomId"].as_str().unwrap();
    let host_id = created["peerId"].as_str().unwrap();
    let invite = created["payload"]["inviteToken"].as_str().unwrap();
    let epoch = created["authorityEpoch"].as_u64().unwrap();

    let (mut guest, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    guest
        .send(Message::Text(
            json!({
                "protocolVersion": 1,
                "messageType": "room.guestHello",
                "payload": {
                    "inviteToken": invite,
                    "nickname": "Guest",
                    "clientInstanceId": "guest-instance-1"
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let joined = next_json(&mut guest).await;
    assert_eq!(joined["messageType"], "relay.roomJoined");
    let guest_id = joined["peerId"].as_str().unwrap();
    let peer_joined = next_json(&mut host).await;
    assert_eq!(peer_joined["messageType"], "room.peerJoined");

    let transform = test_envelope(
        "scene.transformBatch",
        room_id,
        host_id,
        epoch,
        1,
        None,
        json!({ "entities": [{ "id": "object-1", "x": 12.0 }] }),
    );
    host.send(Message::Text(
        serde_json::to_string(&transform).unwrap().into(),
    ))
    .await
    .unwrap();
    let mirrored = next_json(&mut guest).await;
    assert_eq!(mirrored["messageType"], "scene.transformBatch");
    assert_eq!(mirrored["payload"]["entities"][0]["x"], 12.0);

    let begin_grab = test_envelope(
        "interaction.beginGrab",
        room_id,
        guest_id,
        epoch,
        1,
        Some(host_id.to_string()),
        json!({ "entityId": "object-1", "x": 12.0, "y": 20.0 }),
    );
    guest
        .send(Message::Text(
            serde_json::to_string(&begin_grab).unwrap().into(),
        ))
        .await
        .unwrap();
    let intent = next_json(&mut host).await;
    assert_eq!(intent["messageType"], "interaction.beginGrab");
    assert_eq!(intent["peerId"], guest_id);

    relay.abort();
}

#[tokio::test]
async fn guest_cannot_publish_authoritative_scene_state() {
    let (address, relay) = spawn_relay().await;
    let (mut host, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    host.send(Message::Text(
        json!({
            "protocolVersion": 1,
            "messageType": "room.hostHello",
            "payload": { "nickname": "Host", "hostSessionId": "host-session-2" }
        })
        .to_string()
        .into(),
    ))
    .await
    .unwrap();
    let created = next_json(&mut host).await;
    let room_id = created["roomId"].as_str().unwrap();
    let invite = created["payload"]["inviteToken"].as_str().unwrap();
    let epoch = created["authorityEpoch"].as_u64().unwrap();

    let (mut guest, _) = connect_async(format!("ws://{address}/ws")).await.unwrap();
    guest
        .send(Message::Text(
            json!({
                "protocolVersion": 1,
                "messageType": "room.guestHello",
                "payload": {
                    "inviteToken": invite,
                    "nickname": "Guest",
                    "clientInstanceId": "guest-instance-2"
                }
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let joined = next_json(&mut guest).await;
    let guest_id = joined["peerId"].as_str().unwrap();
    let _ = next_json(&mut host).await;

    let forged = test_envelope(
        "scene.transformBatch",
        room_id,
        guest_id,
        epoch,
        1,
        None,
        json!({ "entities": [] }),
    );
    guest
        .send(Message::Text(
            serde_json::to_string(&forged).unwrap().into(),
        ))
        .await
        .unwrap();
    let error = next_json(&mut guest).await;
    assert_eq!(error["messageType"], "room.protocolError");
    assert_eq!(error["payload"]["code"], "permission_denied");

    relay.abort();
}
