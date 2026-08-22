use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::local_relay::{self, RelaySession};
use crate::room_bridge::RoomBridge;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

pub const ROOM_SNAPSHOT_EVENT: &str = "room://snapshot";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomFeatureFilters {
    pub objects: bool,
    pub twitch: bool,
    pub world_effects: bool,
    pub games_and_portals: bool,
    pub imported_assets: bool,
}

impl Default for RoomFeatureFilters {
    fn default() -> Self {
        Self {
            objects: true,
            twitch: true,
            world_effects: true,
            games_and_portals: true,
            imported_assets: false,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomParticipant {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub can_interact: bool,
    pub status: String,
    pub latency_ms: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomSnapshot {
    pub status: String,
    pub role: Option<String>,
    pub room_id: Option<String>,
    pub invite_url: Option<String>,
    pub local_display_name: String,
    pub target_display_id: Option<String>,
    pub allow_guest_interaction: bool,
    pub feature_filters: RoomFeatureFilters,
    pub participants: Vec<RoomParticipant>,
    pub relay_region: Option<String>,
    pub latency_ms: Option<u32>,
    pub last_packet_age_ms: Option<u32>,
    pub retry_in_ms: Option<u32>,
    pub last_error: Option<String>,
    pub local_simulation: bool,
}

impl Default for RoomSnapshot {
    fn default() -> Self {
        Self {
            status: "idle".to_string(),
            role: None,
            room_id: None,
            invite_url: None,
            local_display_name: "Desktop friend".to_string(),
            target_display_id: None,
            allow_guest_interaction: true,
            feature_filters: RoomFeatureFilters::default(),
            participants: Vec::new(),
            relay_region: None,
            latency_ms: None,
            last_packet_age_ms: None,
            retry_in_ms: None,
            last_error: None,
            local_simulation: true,
        }
    }
}

#[derive(Clone, Default)]
pub struct RoomService {
    snapshot: Arc<Mutex<RoomSnapshot>>,
    relay_session: Arc<Mutex<Option<RelayMembership>>>,
}

struct RelayMembership {
    session: RelaySession,
    room_id: String,
    peer_id: String,
    host_peer_id: Option<String>,
    authority_epoch: u64,
    sequence: u64,
}

impl RoomService {
    pub fn start_transport_pump(&self, bridge: RoomBridge) {
        let service = self.clone();
        let _ = thread::Builder::new()
            .name("room-transport-pump".to_string())
            .spawn(move || loop {
                service.pump_transport(&bridge);
                thread::sleep(Duration::from_millis(16));
            });
    }
    pub fn snapshot(&self) -> RoomSnapshot {
        self.snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_default()
    }

    pub fn host(
        &self,
        display_name: String,
        target_display_id: Option<String>,
        allow_guest_interaction: bool,
        feature_filters: RoomFeatureFilters,
    ) -> Result<RoomSnapshot, String> {
        let display_name = normalized_display_name(display_name);
        let relay = local_relay::connect_host(&display_name).ok();
        let room_id = relay
            .as_ref()
            .map(|welcome| welcome.room_id.clone())
            .unwrap_or_else(|| generated_id("room"));
        let invite_key = relay
            .as_ref()
            .and_then(|welcome| welcome.invite_token.clone())
            .unwrap_or_else(|| generated_id("invite"));
        let is_relay_backed = relay.is_some();
        if let Some(relay) = relay {
            *self
                .relay_session
                .lock()
                .map_err(|_| "Relay session lock was poisoned.".to_string())? =
                Some(RelayMembership {
                    session: relay.session,
                    room_id: relay.room_id.clone(),
                    peer_id: relay.peer_id,
                    host_peer_id: None,
                    authority_epoch: relay.authority_epoch,
                    sequence: 0,
                });
        }
        let mut snapshot = self.lock()?;
        *snapshot = RoomSnapshot {
            status: "live".to_string(),
            role: Some("host".to_string()),
            room_id: Some(room_id.clone()),
            invite_url: Some(format!(
                "http://127.0.0.1:8787/join/{room_id}?key={invite_key}"
            )),
            local_display_name: display_name.clone(),
            target_display_id,
            allow_guest_interaction,
            feature_filters,
            participants: vec![RoomParticipant {
                id: "local".to_string(),
                display_name,
                role: "host".to_string(),
                can_interact: true,
                status: "live".to_string(),
                latency_ms: Some(0),
            }],
            relay_region: Some(
                if is_relay_backed {
                    "Local relay"
                } else {
                    "Local simulation"
                }
                .to_string(),
            ),
            latency_ms: Some(0),
            last_packet_age_ms: Some(0),
            retry_in_ms: None,
            last_error: None,
            local_simulation: !is_relay_backed,
        };
        Ok(snapshot.clone())
    }

    pub fn join(
        &self,
        invite_url: String,
        display_name: String,
        target_display_id: Option<String>,
    ) -> Result<RoomSnapshot, String> {
        let room_id = room_id_from_invite(&invite_url)?;
        let invite_token = invite_token_from_invite(&invite_url)?;
        let display_name = normalized_display_name(display_name);
        let relay = local_relay::connect_guest(&room_id, &invite_token, &display_name).ok();
        let is_relay_backed = relay.is_some();
        let host_peer_id = relay
            .as_ref()
            .and_then(|welcome| welcome.host_peer_id.clone())
            .unwrap_or_else(|| "simulated-host".to_string());
        if let Some(relay) = relay {
            let host_peer_id = relay.host_peer_id.clone();
            let mut membership = RelayMembership {
                session: relay.session,
                room_id: relay.room_id.clone(),
                peer_id: relay.peer_id,
                host_peer_id,
                authority_epoch: relay.authority_epoch,
                sequence: 0,
            };
            let keyframe_target = membership.host_peer_id.clone();
            send_relay_frame(
                &mut membership,
                "scene.keyframeRequest",
                serde_json::json!({ "reason": "guest_join", "lastHostTick": 0 }),
                keyframe_target,
            );
            *self
                .relay_session
                .lock()
                .map_err(|_| "Relay session lock was poisoned.".to_string())? = Some(membership);
        }
        let mut snapshot = self.lock()?;
        *snapshot = RoomSnapshot {
            status: "live".to_string(),
            role: Some("guest".to_string()),
            room_id: Some(room_id),
            invite_url: None,
            local_display_name: display_name.clone(),
            target_display_id,
            allow_guest_interaction: true,
            feature_filters: RoomFeatureFilters::default(),
            participants: vec![
                RoomParticipant {
                    id: host_peer_id,
                    display_name: "Room host".to_string(),
                    role: "host".to_string(),
                    can_interact: true,
                    status: "live".to_string(),
                    latency_ms: Some(18),
                },
                RoomParticipant {
                    id: "local".to_string(),
                    display_name,
                    role: "guest".to_string(),
                    can_interact: true,
                    status: "live".to_string(),
                    latency_ms: Some(0),
                },
            ],
            relay_region: Some(
                if is_relay_backed {
                    "Local relay"
                } else {
                    "Local simulation"
                }
                .to_string(),
            ),
            latency_ms: Some(18),
            last_packet_age_ms: Some(4),
            retry_in_ms: None,
            last_error: None,
            local_simulation: !is_relay_backed,
        };
        Ok(snapshot.clone())
    }

    pub fn leave(&self) -> Result<RoomSnapshot, String> {
        *self
            .relay_session
            .lock()
            .map_err(|_| "Relay session lock was poisoned.".to_string())? = None;
        let mut snapshot = self.lock()?;
        let display_name = snapshot.local_display_name.clone();
        let target_display_id = snapshot.target_display_id.clone();
        let feature_filters = snapshot.feature_filters.clone();
        let allow_guest_interaction = snapshot.allow_guest_interaction;
        *snapshot = RoomSnapshot {
            local_display_name: display_name,
            target_display_id,
            feature_filters,
            allow_guest_interaction,
            ..RoomSnapshot::default()
        };
        Ok(snapshot.clone())
    }

    pub fn set_interaction(&self, enabled: bool) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        ensure_host(&snapshot)?;
        snapshot.allow_guest_interaction = enabled;
        for participant in &mut snapshot.participants {
            if participant.role == "guest" {
                participant.can_interact = enabled;
            }
        }
        Ok(snapshot.clone())
    }

    pub fn set_participant_interaction(
        &self,
        participant_id: String,
        enabled: bool,
    ) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        ensure_host(&snapshot)?;
        let participant = snapshot
            .participants
            .iter_mut()
            .find(|participant| participant.id == participant_id)
            .ok_or_else(|| "Participant is no longer in the room.".to_string())?;
        if participant.role == "host" {
            return Err("The host always retains interaction control.".to_string());
        }
        participant.can_interact = enabled;
        Ok(snapshot.clone())
    }

    pub fn remove_participant(&self, participant_id: String) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        ensure_host(&snapshot)?;
        snapshot
            .participants
            .retain(|participant| participant.id == "local" || participant.id != participant_id);
        Ok(snapshot.clone())
    }

    pub fn set_features(
        &self,
        feature_filters: RoomFeatureFilters,
    ) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        ensure_host(&snapshot)?;
        snapshot.feature_filters = feature_filters;
        Ok(snapshot.clone())
    }

    pub fn regenerate_invite(&self) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        ensure_host(&snapshot)?;
        let room_id = snapshot
            .room_id
            .clone()
            .ok_or_else(|| "No room is active.".to_string())?;
        snapshot.invite_url = Some(format!(
            "https://rooms.screenoverlayphysics.local/join/{room_id}?key={}",
            generated_id("invite")
        ));
        Ok(snapshot.clone())
    }

    pub fn reconnect(&self) -> Result<RoomSnapshot, String> {
        let mut snapshot = self.lock()?;
        if snapshot.room_id.is_none() {
            return Err("No room is available to reconnect.".to_string());
        }
        snapshot.status = "live".to_string();
        snapshot.retry_in_ms = None;
        snapshot.last_error = None;
        snapshot.last_packet_age_ms = Some(0);
        for participant in &mut snapshot.participants {
            if participant.id == "local" {
                participant.status = "live".to_string();
            }
        }
        Ok(snapshot.clone())
    }

    /// Transfers frames between the persistent native bridge and the real
    /// localhost relay. This runs from the UI snapshot command today; the
    /// next hardening pass will move it to its own service timer.
    pub fn pump_transport(&self, bridge: &RoomBridge) {
        let Ok(mut relay) = self.relay_session.lock() else {
            return;
        };
        let Some(relay) = relay.as_mut() else {
            return;
        };
        for native in bridge.drain() {
            let message_type = match native.kind.as_str() {
                "scene.keyframe" => "scene.keyframe",
                "scene.transformBatch" => "scene.transformBatch",
                "interaction.beginGrab" => "interaction.beginGrab",
                "interaction.moveGrab" => "interaction.moveGrab",
                "interaction.endGrab" => "interaction.endGrab",
                "interaction.spawnIntent" => "interaction.spawnIntent",
                _ => continue,
            };
            if !matches!(
                message_type,
                "scene.transformBatch" | "interaction.moveGrab"
            ) {
                crate::room_trace(format!("pump native->relay type={message_type}"));
            }
            send_relay_frame(relay, message_type, native.payload, None);
        }
        while let Ok(frame) = relay.session.incoming.try_recv() {
            let message_type = frame
                .get("messageType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !matches!(
                message_type.as_str(),
                "scene.transformBatch" | "interaction.moveGrab"
            ) {
                crate::room_trace(format!("pump relay->native type={message_type}"));
            }
            if matches!(
                message_type.as_str(),
                "scene.keyframe"
                    | "scene.transformBatch"
                    | "interaction.beginGrab"
                    | "interaction.moveGrab"
                    | "interaction.endGrab"
                    | "interaction.spawnIntent"
            ) {
                bridge.send("relayFrame", frame);
            }
            if message_type == "scene.keyframeRequest" {
                bridge.send("requestKeyframe", serde_json::Value::Null);
            }
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RoomSnapshot>, String> {
        self.snapshot
            .lock()
            .map_err(|_| "Room state lock was poisoned.".to_string())
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn send_relay_frame(
    relay: &mut RelayMembership,
    message_type: &str,
    payload: serde_json::Value,
    target_peer_id: Option<String>,
) {
    relay.sequence = relay.sequence.saturating_add(1);
    let host_tick = payload
        .get("hostTick")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let frame = serde_json::json!({ "protocolVersion": 1, "messageType": message_type, "roomId": relay.room_id, "peerId": relay.peer_id, "authorityEpoch": relay.authority_epoch, "sequence": relay.sequence, "hostTick": host_tick, "sentAtUnixMs": unix_time_ms(), "targetPeerId": target_peer_id, "payload": payload });
    let _ = relay.session.outgoing.try_send(frame);
}

pub fn emit_snapshot(app: &AppHandle, snapshot: &RoomSnapshot) {
    let _ = app.emit(ROOM_SNAPSHOT_EVENT, snapshot.clone());
}

fn normalized_display_name(display_name: String) -> String {
    let trimmed = display_name.trim();
    if trimmed.is_empty() {
        "Desktop friend".to_string()
    } else {
        trimmed.chars().take(32).collect()
    }
}

fn ensure_host(snapshot: &RoomSnapshot) -> Result<(), String> {
    if snapshot.role.as_deref() == Some("host") && snapshot.status != "idle" {
        Ok(())
    } else {
        Err("Only the room host can change this setting.".to_string())
    }
}

fn generated_id(prefix: &str) -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros())
        .unwrap_or_default();
    format!("{prefix}-{micros:x}")
}

fn room_id_from_invite(invite_url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(invite_url.trim())
        .map_err(|_| "Paste a complete room invite URL.".to_string())?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Room invites must use an HTTP or HTTPS URL.".to_string());
    }
    let segments = parsed
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let room_id = segments
        .windows(2)
        .find(|parts| parts[0] == "join")
        .map(|parts| parts[1])
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "This does not look like a room invite URL.".to_string())?;
    Ok(room_id.to_string())
}

fn invite_token_from_invite(invite_url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(invite_url.trim())
        .map_err(|_| "Paste a complete room invite URL.".to_string())?;
    let token = parsed
        .query_pairs()
        .find(|(key, _)| key == "key")
        .map(|(_, value)| value.into_owned())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "Room invite does not include its capability key.".to_string())?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_can_update_room_controls() {
        let service = RoomService::default();
        service
            .host(
                "Alex".to_string(),
                Some("display-1".to_string()),
                true,
                RoomFeatureFilters::default(),
            )
            .expect("host room");
        let snapshot = service.set_interaction(false).expect("set interaction");
        assert!(!snapshot.allow_guest_interaction);
        assert_eq!(snapshot.role.as_deref(), Some("host"));
    }

    #[test]
    fn invite_parser_requires_join_path() {
        assert_eq!(
            room_id_from_invite("https://example.test/join/family-room?key=secret")
                .expect("valid invite"),
            "family-room"
        );
        assert!(room_id_from_invite("https://example.test/rooms/family-room").is_err());
    }

    #[test]
    fn leaving_preserves_local_preferences() {
        let service = RoomService::default();
        service
            .host(
                "Mina".to_string(),
                Some("display-2".to_string()),
                false,
                RoomFeatureFilters::default(),
            )
            .expect("host room");
        let snapshot = service.leave().expect("leave room");
        assert_eq!(snapshot.status, "idle");
        assert_eq!(snapshot.local_display_name, "Mina");
        assert_eq!(snapshot.target_display_id.as_deref(), Some("display-2"));
        assert!(!snapshot.allow_guest_interaction);
    }
}
