//! Versioned, renderer-independent wire types for Shared Desktop Rooms.
//!
//! This crate intentionally contains only DTOs and JSON codec rules.  Runtime
//! policy (permissions, rate limits, simulation, and transport) belongs to the
//! native app, Tauri room service, or relay.

pub mod codec;
pub mod envelope;
pub mod identity;
pub mod interaction;
pub mod room;
pub mod scene;

pub use codec::{MAX_ENVELOPE_BYTES, ProtocolCodecError, decode_envelope, encode_envelope};
pub use envelope::{Envelope, MessageType, PROTOCOL_VERSION};
pub use identity::{AuthorityEpoch, EntityRevision, NetworkEntityId, PeerId, RoomId, SurfaceId};
pub use interaction::*;
pub use room::*;
pub use scene::*;
