use std::collections::HashMap;

use axum::extract::ws::Message;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::protocol::PeerRole;

pub const MAX_ROOM_PARTICIPANTS: usize = 8;
pub const PEER_QUEUE_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct PeerRecord {
    pub peer_id: String,
    pub nickname: String,
    pub role: PeerRole,
    pub sender: mpsc::Sender<Message>,
}

pub struct Room {
    pub room_id: String,
    pub invite_hash: [u8; 32],
    pub authority_epoch: u64,
    pub host: PeerRecord,
    pub guests: HashMap<String, PeerRecord>,
}

impl Room {
    pub fn participant_count(&self) -> usize {
        1 + self.guests.len()
    }

    pub fn contains_peer(&self, peer_id: &str) -> bool {
        self.host.peer_id == peer_id || self.guests.contains_key(peer_id)
    }
}

#[derive(Default)]
pub struct RoomRegistry {
    rooms: HashMap<String, Room>,
}

pub struct CreatedRoom {
    pub room_id: String,
    pub invite_token: String,
    pub peer_id: String,
    pub authority_epoch: u64,
}

pub struct JoinedRoom {
    pub peer_id: String,
    pub host_peer_id: String,
    pub authority_epoch: u64,
    pub participant_count: usize,
}

pub struct DisconnectOutcome {
    pub room_ended: bool,
    pub remaining: Vec<mpsc::Sender<Message>>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("room was not found")]
    RoomNotFound,
    #[error("invite token is invalid")]
    InvalidInvite,
    #[error("room is full")]
    RoomFull,
    #[error("peer is not a member of the room")]
    PeerNotFound,
    #[error("target peer is not a member of the room")]
    TargetNotFound,
    #[error("guest messages may only target the host")]
    InvalidGuestTarget,
}

impl RoomRegistry {
    pub fn create_room(&mut self, nickname: String, sender: mpsc::Sender<Message>) -> CreatedRoom {
        let room_id = Uuid::new_v4().simple().to_string()[..12].to_string();
        let peer_id = peer_id();
        let invite_secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let invite_token = format!("{room_id}.{invite_secret}");
        let authority_epoch = 1;
        let room = Room {
            room_id: room_id.clone(),
            invite_hash: hash_token(&invite_token),
            authority_epoch,
            host: PeerRecord {
                peer_id: peer_id.clone(),
                nickname,
                role: PeerRole::Host,
                sender,
            },
            guests: HashMap::new(),
        };
        self.rooms.insert(room_id.clone(), room);
        CreatedRoom {
            room_id,
            invite_token,
            peer_id,
            authority_epoch,
        }
    }

    pub fn join_room(
        &mut self,
        room_id: &str,
        invite_token: &str,
        nickname: String,
        sender: mpsc::Sender<Message>,
    ) -> Result<JoinedRoom, RegistryError> {
        let room = self
            .rooms
            .get_mut(room_id)
            .ok_or(RegistryError::RoomNotFound)?;
        let candidate_hash = hash_token(invite_token);
        if !bool::from(room.invite_hash.ct_eq(&candidate_hash)) {
            return Err(RegistryError::InvalidInvite);
        }
        if room.participant_count() >= MAX_ROOM_PARTICIPANTS {
            return Err(RegistryError::RoomFull);
        }

        let peer_id = peer_id();
        room.guests.insert(
            peer_id.clone(),
            PeerRecord {
                peer_id: peer_id.clone(),
                nickname,
                role: PeerRole::Guest,
                sender,
            },
        );
        Ok(JoinedRoom {
            peer_id,
            host_peer_id: room.host.peer_id.clone(),
            authority_epoch: room.authority_epoch,
            participant_count: room.participant_count(),
        })
    }

    pub fn room(&self, room_id: &str) -> Option<&Room> {
        self.rooms.get(room_id)
    }

    pub fn forward_targets(
        &self,
        room_id: &str,
        sender_peer_id: &str,
        role: PeerRole,
        target_peer_id: Option<&str>,
    ) -> Result<Vec<mpsc::Sender<Message>>, RegistryError> {
        let room = self.rooms.get(room_id).ok_or(RegistryError::RoomNotFound)?;
        if !room.contains_peer(sender_peer_id) {
            return Err(RegistryError::PeerNotFound);
        }

        match role {
            PeerRole::Host => match target_peer_id {
                Some(target) => room
                    .guests
                    .get(target)
                    .map(|peer| vec![peer.sender.clone()])
                    .ok_or(RegistryError::TargetNotFound),
                None => Ok(room
                    .guests
                    .values()
                    .map(|peer| peer.sender.clone())
                    .collect()),
            },
            PeerRole::Guest => {
                if target_peer_id.is_some_and(|target| target != room.host.peer_id) {
                    return Err(RegistryError::InvalidGuestTarget);
                }
                Ok(vec![room.host.sender.clone()])
            }
        }
    }

    pub fn disconnect(
        &mut self,
        room_id: &str,
        peer_id: &str,
        role: PeerRole,
    ) -> DisconnectOutcome {
        if role == PeerRole::Host {
            let remaining = self
                .rooms
                .remove(room_id)
                .map(|room| room.guests.into_values().map(|peer| peer.sender).collect())
                .unwrap_or_default();
            return DisconnectOutcome {
                room_ended: true,
                remaining,
            };
        }

        let Some(room) = self.rooms.get_mut(room_id) else {
            return DisconnectOutcome {
                room_ended: false,
                remaining: Vec::new(),
            };
        };
        room.guests.remove(peer_id);
        DisconnectOutcome {
            room_ended: false,
            remaining: vec![room.host.sender.clone()],
        }
    }
}

fn hash_token(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn peer_id() -> String {
    Uuid::new_v4().simple().to_string()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sender() -> mpsc::Sender<Message> {
        mpsc::channel(1).0
    }

    #[test]
    fn invite_is_required_and_guest_targets_only_host() {
        let mut registry = RoomRegistry::default();
        let created = registry.create_room("host".into(), sender());
        assert!(matches!(
            registry.join_room(&created.room_id, "wrong", "guest".into(), sender()),
            Err(RegistryError::InvalidInvite)
        ));
        let joined = registry
            .join_room(
                &created.room_id,
                &created.invite_token,
                "guest".into(),
                sender(),
            )
            .unwrap();
        assert!(registry
            .forward_targets(
                &created.room_id,
                &joined.peer_id,
                PeerRole::Guest,
                Some(&created.peer_id),
            )
            .is_ok());
        assert_eq!(
            registry
                .forward_targets(
                    &created.room_id,
                    &joined.peer_id,
                    PeerRole::Guest,
                    Some("another-guest"),
                )
                .err(),
            Some(RegistryError::InvalidGuestTarget)
        );
    }
}
