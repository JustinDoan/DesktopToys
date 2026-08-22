//! CS:GO-style case opening for the overlay.
//!
//! This crate owns the whole simulation: the rarity ladder and its odds, the
//! seeded roll, the reel strip, the particle system, a virtual camera, and the
//! phase machine that choreographs them. It draws nothing. The `renderer`
//! crate reads [`CaseSession::stage`] and turns it into triangles, and the app
//! feeds it delta time and viewer requests.
//!
//! Configs use the same `rewards.json` schema as the browser-source overlay at
//! `zedandgone/csgo-case-overlay`, so an existing dashboard file loads as-is.
//!
//! ```
//! use case_sim::{CaseConfig, CaseRequest, CaseSession};
//!
//! let mut session = CaseSession::new(
//!     CaseConfig::default(),
//!     CaseRequest::for_viewer("zedandgone"),
//! );
//! while !session.is_finished() {
//!     session.update(1.0 / 60.0);
//! }
//! println!("{}", session.result().reward_name);
//! ```

pub mod celebration;
pub mod config;
pub mod ease;
pub mod math;
pub mod particles;
pub mod rng;
pub mod session;

pub use celebration::Celebration;
pub use config::{CaseConfig, HypeMessages, LegendaryWheel, RarityTier, Reward, RollOutcome};
pub use math::V3;
pub use particles::{MAX_PARTICLES, Particle, ParticleKind, ParticleSystem, Shockwave};
pub use rng::Rng;
pub use session::{
    CARD_DEPTH, CARD_HEIGHT, CARD_PITCH, CARD_WIDTH, CameraRig, CasePhase, CaseRequest, CaseResult,
    CaseSession, CaseStage, PRIZE_ORIGIN, REEL_HALF_SPAN, REEL_RADIUS, ReelSlot, WHEEL_CENTER,
    WHEEL_PANEL_HALF, wheel_radius,
};

use std::path::Path;

impl CaseConfig {
    /// Loads a dashboard `rewards.json` from disk.
    pub fn load(path: &Path, profile_id: Option<&str>) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::from_rewards_json(&text, profile_id)
    }
}
