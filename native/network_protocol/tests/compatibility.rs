use network_protocol::*;
use serde_json::json;

fn envelope() -> Envelope {
    let mut envelope = Envelope::new(
        MessageType::TransformBatch,
        "room-7".into(),
        "host-1".into(),
        json!({"hostTick": 4, "transforms": []}),
    );
    envelope.authority_epoch = 2;
    envelope.sequence = 8;
    envelope.host_tick = 4;
    envelope.sent_at_unix_ms = 1_784_600_000_000;
    envelope
}

#[test]
fn golden_envelope_uses_stable_camel_case_and_message_name() {
    let json: serde_json::Value =
        serde_json::from_slice(&encode_envelope(&envelope()).unwrap()).unwrap();
    assert_eq!(json["protocolVersion"], 1);
    assert_eq!(json["messageType"], "scene.transformBatch");
    assert_eq!(json["authorityEpoch"], 2);
    assert!(json.get("targetPeerId").is_none());
}

#[test]
fn envelope_round_trip_preserves_payload_and_target() {
    let mut original = envelope();
    original.target_peer_id = Some("guest-2".into());
    assert_eq!(
        decode_envelope(&encode_envelope(&original).unwrap()).unwrap(),
        original
    );
}

#[test]
fn unknown_protocol_is_rejected() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&encode_envelope(&envelope()).unwrap()).unwrap();
    value["protocolVersion"] = json!(99);
    assert!(matches!(
        decode_envelope(value.to_string().as_bytes()),
        Err(ProtocolCodecError::UnsupportedProtocol { received: 99, .. })
    ));
}

#[test]
fn stable_error_and_role_tags_are_snake_case_and_camel_case() {
    assert_eq!(
        serde_json::to_string(&ProtocolErrorCodeV1::PermissionDenied).unwrap(),
        "\"permission_denied\""
    );
    assert_eq!(
        serde_json::to_string(&RoomRoleV1::Guest).unwrap(),
        "\"guest\""
    );
}

#[test]
fn maximum_payload_is_enforced_before_parsing() {
    let oversized = vec![b' '; MAX_ENVELOPE_BYTES + 1];
    assert!(matches!(
        decode_envelope(&oversized),
        Err(ProtocolCodecError::PayloadTooLarge { .. })
    ));
}
