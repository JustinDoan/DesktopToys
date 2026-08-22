//! The opening sequence: one state machine that drives every animated value
//! the renderer reads.
//!
//! Nothing here touches the GPU. The session owns timing, the roll, particle
//! emission and a virtual camera; the renderer turns that into triangles.

use core_types::AppColor;

use crate::celebration::{Celebration, CelebrationScene};
use crate::config::{CaseConfig, RollOutcome};
use crate::ease::{
    self, REEL_CURVE, WHEEL_CURVE, clamp01, ease_out_back, ease_out_cubic, ease_out_elastic,
    ease_out_quint, lerp, smootherstep, smoothstep,
};
use crate::math::V3;
use crate::particles::{ParticleSystem, Shockwave};
use crate::rng::Rng;

/// Card pitch and size in case-space units, which map roughly to pixels at the
/// reel's depth.
pub const CARD_WIDTH: f32 = 216.0;
pub const CARD_HEIGHT: f32 = 252.0;
pub const CARD_DEPTH: f32 = 18.0;
pub const CARD_PITCH: f32 = CARD_WIDTH + 16.0;
/// Radius of the cylinder the reel is wrapped around. Large enough that the
/// curve is felt rather than seen.
pub const REEL_RADIUS: f32 = 1320.0;
/// Half-width of the reel window; cards outside are culled. Sized to keep about
/// six cards on screen at the wider card pitch.
pub const REEL_HALF_SPAN: f32 = 760.0;

/// Where the won prize sits once it has ridden forward, and so where the
/// celebration is centred.
pub const PRIZE_ORIGIN: V3 = V3::new(0.0, 0.0, 60.0);

/// Radius of the legendary wheel's carousel, and the half-extents of one slot.
/// The renderer lays the wheel out from these, and the session uses them to put
/// the landing celebration on the winning slot.
pub const WHEEL_PANEL_HALF: V3 = V3::new(196.0, 116.0, 9.0);
pub const WHEEL_CENTER: V3 = V3::new(0.0, -60.0, 0.0);

/// Spacing between neighbouring slots, so a wheel with many prizes grows rather
/// than letting its panels intersect.
pub fn wheel_radius(prize_count: usize) -> f32 {
    let count = prize_count.max(3) as f32;
    let clearance = WHEEL_PANEL_HALF.x * 2.0 * 1.3;
    (clearance * 0.5 / (std::f32::consts::PI / count).sin()).max(430.0)
}

/// The slot the wheel's pointer sits over, in case space.
pub fn wheel_landing_position(prize_count: usize) -> V3 {
    WHEEL_CENTER + V3::new(0.0, 0.0, wheel_radius(prize_count))
}

/// Where the winning card sits in the strip, and how long the strip is. Matched
/// to the browser overlay so the pacing feels identical.
const WINNER_SLOT: usize = 58;
const REEL_SLOTS: usize = 74;

const INTRO_SECONDS: f32 = 1.15;
const CHARGE_SECONDS: f32 = 1.40;
const BURST_SECONDS: f32 = 0.40;
const SPIN_SECONDS: f32 = 7.20;
const REVEAL_SECONDS: f32 = 1.30;
const BASE_HOLD_SECONDS: f32 = 5.20;
const EXTENDED_HOLD_BONUS: f32 = 3.50;
/// Shortened hold when a legendary wheel is queued up behind it.
const PRE_WHEEL_HOLD_SECONDS: f32 = 2.10;
/// The least time a decided prize stays on screen, counted from the moment it
/// is legible. Applies to both reveals: the card and the wheel's landing.
const RESULT_HOLD_FLOOR_SECONDS: f32 = 5.00;
/// How long the winning card takes to clear out at the top of the wheel phase.
const WHEEL_HANDOFF_SECONDS: f32 = 0.55;
/// The wheel's landing is the loudest moment in the sequence, so it runs the
/// tier's recipe harder than the first reveal did.
const WHEEL_LANDING_FORCE: f32 = 1.7;
/// Fraction of the spin at which the wheel counts as landed. The deceleration
/// curve is flat over its last stretch, so waiting for the clock to run out
/// would fire the celebration a beat after the wheel visibly stopped.
const WHEEL_SETTLE_AT: f32 = 0.86;
const OUTRO_SECONDS: f32 = 0.65;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasePhase {
    /// Case flies in from depth.
    Intro,
    /// Case hovers and charges, seams lighting up.
    Charge,
    /// Case cracks open and the reel materialises.
    Burst,
    /// The reel scrolls and decelerates.
    Spin,
    /// The winning card pops out of the strip.
    Reveal,
    /// The prize sits on screen to be read.
    Hold,
    /// Top-tier bonus wheel.
    Wheel,
    /// Everything shrinks away.
    Outro,
    Done,
}

impl CasePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Intro => "INTRO",
            Self::Charge => "CHARGE",
            Self::Burst => "BURST",
            Self::Spin => "SPIN",
            Self::Reveal => "REVEAL",
            Self::Hold => "HOLD",
            Self::Wheel => "WHEEL",
            Self::Outro => "OUTRO",
            Self::Done => "DONE",
        }
    }
}

/// What the caller asked for.
#[derive(Clone, Debug, Default)]
pub struct CaseRequest {
    pub viewer: String,
    pub forced_tier: Option<String>,
    pub forced_reward: Option<String>,
    pub seed: Option<u64>,
    /// Consecutive openings by this viewer; shown next to their name.
    pub streak: u32,
}

impl CaseRequest {
    pub fn for_viewer(viewer: &str) -> Self {
        Self {
            viewer: viewer.to_string(),
            ..Self::default()
        }
    }
}

/// The settled drop, for logging or forwarding back to chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseResult {
    pub viewer: String,
    pub tier_id: String,
    pub tier_name: String,
    pub reward_name: String,
    pub wheel_prize: Option<String>,
}

/// A single card in the strip.
#[derive(Clone, Copy, Debug)]
pub struct ReelSlot {
    pub tier_index: usize,
    pub reward_index: usize,
    /// Small per-card tilt so the strip doesn't look mechanically flat.
    pub tilt: f32,
    /// Random phase for the card's holographic sheen.
    pub seed: f32,
}

/// Virtual camera. The renderer projects with this; the overlay's own
/// perspective is bypassed so the case can own its framing.
#[derive(Clone, Copy, Debug)]
pub struct CameraRig {
    pub eye: V3,
    pub target: V3,
    pub up: V3,
    pub fov_y: f32,
    /// Screen-space roll in radians, applied after projection.
    pub roll: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            eye: V3::new(0.0, 40.0, 1450.0),
            target: V3::ZERO,
            up: V3::new(0.0, 1.0, 0.0),
            fov_y: 46.0f32.to_radians(),
            roll: 0.0,
        }
    }
}

/// Every animated value the renderer needs, recomputed once per frame.
#[derive(Clone, Debug)]
pub struct CaseStage {
    pub phase: CasePhase,
    /// Progress through the current phase, 0..1.
    pub phase_progress: f32,
    pub elapsed: f32,
    pub camera: CameraRig,

    /// Screen darkening behind the whole sequence, 0..1.
    pub backdrop: f32,
    /// Full-screen flash, 0..1.
    pub flash: f32,
    pub flash_color: AppColor,
    /// Multiplies every emitted alpha, for the outro.
    pub global_fade: f32,

    pub case_visible: bool,
    pub case_position: V3,
    /// Euler angles in radians, applied X then Y then Z.
    pub case_rotation: V3,
    pub case_scale: f32,
    pub case_opacity: f32,
    /// Lid hinge angle in radians; 0 is shut.
    pub case_lid_angle: f32,
    /// How hot the panel seams glow, 0..1.
    pub case_seam_glow: f32,
    /// Vertical light pillar out of the open case, 0..1.
    pub case_pillar: f32,

    pub reel_visible: bool,
    pub reel_scroll: f32,
    pub reel_velocity: f32,
    pub reel_opacity: f32,
    /// Fades the strip in as it materialises, 0..1.
    pub reel_assemble: f32,
    /// One-shot flash on the centre ticker, 0..1.
    pub ticker_flash: f32,

    /// Winning card's pop-out, 0..1.
    pub winner_pop: f32,
    /// How far the winning card travels toward the camera.
    pub winner_lift: f32,
    /// Non-winning cards falling away during the reveal, 0..1.
    pub debris_fall: f32,
    /// God rays behind the winning card, 0..1.
    pub rays: f32,
    /// Reveals the result text block, 0..1.
    pub result_reveal: f32,

    pub wheel_visible: bool,
    pub wheel_angle: f32,
    pub wheel_glow: f32,
    pub wheel_reveal: f32,
    pub wheel_scale: f32,
}

impl Default for CaseStage {
    fn default() -> Self {
        Self {
            phase: CasePhase::Intro,
            phase_progress: 0.0,
            elapsed: 0.0,
            camera: CameraRig::default(),
            backdrop: 0.0,
            flash: 0.0,
            flash_color: AppColor::from_rgb(255, 255, 255),
            global_fade: 1.0,
            case_visible: true,
            case_position: V3::ZERO,
            case_rotation: V3::ZERO,
            case_scale: 1.0,
            case_opacity: 1.0,
            case_lid_angle: 0.0,
            case_seam_glow: 0.0,
            case_pillar: 0.0,
            reel_visible: false,
            reel_scroll: 0.0,
            reel_velocity: 0.0,
            reel_opacity: 0.0,
            reel_assemble: 0.0,
            ticker_flash: 0.0,
            winner_pop: 0.0,
            winner_lift: 0.0,
            debris_fall: 0.0,
            rays: 0.0,
            result_reveal: 0.0,
            wheel_visible: false,
            wheel_angle: 0.0,
            wheel_glow: 0.0,
            wheel_reveal: 0.0,
            wheel_scale: 0.0,
        }
    }
}

pub struct CaseSession {
    config: CaseConfig,
    request: CaseRequest,
    rng: Rng,
    outcome: RollOutcome,
    wheel_prize_index: Option<usize>,

    pub stage: CaseStage,
    pub particles: ParticleSystem,
    pub reel: Vec<ReelSlot>,

    phase_time: f32,
    /// Target reel offset for the winning card to land under the ticker.
    reel_target: f32,
    reel_start: f32,
    ticks_crossed: i64,
    /// Rolling accumulator for ambient dust and streak emission.
    dust_timer: f32,
    streak_timer: f32,
    ember_timer: f32,
    /// Set once the wheel has been decided, so the hold shortens ahead of it.
    wheel_pending: bool,
    /// Ray and card levels captured as the outro begins, so the final fade
    /// starts from what was actually on screen.
    rays_at_outro: f32,
    result_at_outro: f32,
    /// The rolled tier's celebration, chosen once so the reveal and the hold
    /// keep the same character.
    celebration: Celebration,
    /// Drives [`Celebration::beat`] while a prize is on screen.
    beat_timer: f32,
    beats: u32,
    /// One-shot guard for the wheel's own landing celebration.
    wheel_celebrated: bool,
}

impl CaseSession {
    pub fn new(config: CaseConfig, request: CaseRequest) -> Self {
        let seed = request.seed.unwrap_or_else(default_seed);
        let mut rng = Rng::from_seed(seed);
        let outcome = config.roll(
            &mut rng,
            request.forced_tier.as_deref(),
            request.forced_reward.as_deref(),
        );

        let wheel_pending = config.is_top_tier(outcome.tier_index)
            && config.legendary_wheel_enabled
            && config.wheel.is_some();
        let wheel_prize_index = wheel_pending.then(|| {
            let count = config.wheel.as_ref().map(|w| w.prizes.len()).unwrap_or(0);
            rng.below(count.max(1))
        });

        let mut reel = Vec::with_capacity(REEL_SLOTS);
        for slot in 0..REEL_SLOTS {
            let filler = if slot == WINNER_SLOT {
                outcome
            } else {
                config.random_any(&mut rng)
            };
            reel.push(ReelSlot {
                tier_index: filler.tier_index,
                reward_index: filler.reward_index,
                tilt: rng.signed() * 0.03,
                seed: rng.unit(),
            });
        }

        // A little jitter keeps the winner from landing dead centre every time,
        // which is what makes a close call feel close.
        let jitter = rng.signed() * CARD_WIDTH * 0.30;
        let reel_target = WINNER_SLOT as f32 * CARD_PITCH + jitter;

        let celebration = Celebration::for_intensity(
            config.tiers[outcome.tier_index].intensity(config.tiers.len()),
        );

        let mut session = Self {
            config,
            request,
            rng,
            outcome,
            wheel_prize_index,
            stage: CaseStage::default(),
            particles: ParticleSystem::new(),
            reel,
            phase_time: 0.0,
            reel_target,
            reel_start: 0.0,
            ticks_crossed: 0,
            dust_timer: 0.0,
            streak_timer: 0.0,
            ember_timer: 0.0,
            wheel_pending,
            rays_at_outro: 0.0,
            result_at_outro: 0.0,
            celebration,
            beat_timer: 0.0,
            beats: 0,
            wheel_celebrated: false,
        };
        session.enter_phase(CasePhase::Intro);
        session
    }

    pub fn config(&self) -> &CaseConfig {
        &self.config
    }

    pub fn request(&self) -> &CaseRequest {
        &self.request
    }

    pub fn outcome(&self) -> RollOutcome {
        self.outcome
    }

    pub fn winner_slot(&self) -> usize {
        WINNER_SLOT
    }

    pub fn is_finished(&self) -> bool {
        self.stage.phase == CasePhase::Done
    }

    /// True once the drop has been shown, so a caller can report it before the
    /// sequence finishes playing out.
    pub fn result_is_settled(&self) -> bool {
        matches!(
            self.stage.phase,
            CasePhase::Hold | CasePhase::Wheel | CasePhase::Outro | CasePhase::Done
        )
    }

    pub fn tier(&self) -> &crate::config::RarityTier {
        &self.config.tiers[self.outcome.tier_index]
    }

    pub fn tier_color(&self) -> AppColor {
        self.tier().color
    }

    /// 0 for the floor tier, 1 for the top. Scales nearly every effect.
    pub fn intensity(&self) -> f32 {
        self.tier().intensity(self.config.tiers.len())
    }

    /// The celebration recipe this drop earned.
    pub fn celebration(&self) -> Celebration {
        self.celebration
    }

    pub fn result(&self) -> CaseResult {
        CaseResult {
            viewer: self.viewer_name().to_string(),
            tier_id: self.tier().id.clone(),
            tier_name: self.tier().name.clone(),
            reward_name: self.config.reward(self.outcome).name.clone(),
            wheel_prize: self.wheel_prize().map(|prize| prize.name.clone()),
        }
    }

    pub fn wheel_prize(&self) -> Option<&crate::config::Reward> {
        let index = self.wheel_prize_index?;
        self.config.wheel.as_ref()?.prizes.get(index)
    }

    pub fn wheel_prize_index(&self) -> Option<usize> {
        self.wheel_prize_index
    }

    /// The slot the legendary wheel is set to land on, if this run has one.
    pub fn wheel_prize_name(&self) -> Option<&str> {
        let index = self.wheel_prize_index?;
        let prize = self.config.wheel.as_ref()?.prizes.get(index)?;
        Some(prize.name.as_str())
    }

    pub fn viewer_name(&self) -> &str {
        if self.request.viewer.trim().is_empty() {
            "VIEWER"
        } else {
            &self.request.viewer
        }
    }

    // ----- text lines the renderer stamps onto the stage -----

    pub fn header_line(&self) -> String {
        let mut line = format!("{} OPENED", self.viewer_name().to_uppercase());
        if self.request.streak > 1 {
            line.push_str(&format!("  ::  STREAK X{}", self.request.streak));
        }
        line
    }

    pub fn case_title(&self) -> String {
        self.config.case_name.to_uppercase()
    }

    /// The hype line under the header, which escalates with the phase.
    pub fn status_line(&self) -> String {
        match self.stage.phase {
            CasePhase::Intro | CasePhase::Charge | CasePhase::Burst | CasePhase::Spin => {
                self.config.hype.opening.clone()
            }
            CasePhase::Wheel => self
                .config
                .wheel
                .as_ref()
                .map(|wheel| wheel.title.to_uppercase())
                .unwrap_or_else(|| self.config.hype.legendary.clone()),
            _ => {
                let intensity = self.intensity();
                if intensity >= 0.99 {
                    self.config.hype.legendary.clone()
                } else if intensity >= 0.5 {
                    self.config.hype.rare.clone()
                } else {
                    self.config.hype.common.clone()
                }
            }
        }
    }

    pub fn tier_line(&self) -> String {
        let tier = self.tier();
        let mut line = tier.name.to_uppercase();
        if self.config.show_odds {
            line.push_str(&format!("  {:.2}%", tier.odds));
        }
        if self.config.show_profile_name {
            line = format!("{}  /  {line}", self.config.profile_name.to_uppercase());
        }
        line
    }

    pub fn reward_name(&self) -> String {
        self.config.reward(self.outcome).name.to_uppercase()
    }

    pub fn reward_description(&self) -> String {
        self.config.reward(self.outcome).description.clone()
    }

    // ----- simulation -----

    pub fn update(&mut self, dt: f32) {
        if self.stage.phase == CasePhase::Done {
            return;
        }
        // Clamp so a stalled frame (dragging a window, waking from sleep)
        // cannot teleport the reel past its landing.
        let dt = dt.clamp(0.0, 1.0 / 20.0);
        self.stage.elapsed += dt;
        self.phase_time += dt;

        while self.phase_time >= self.phase_duration() && self.stage.phase != CasePhase::Done {
            let overflow = self.phase_time - self.phase_duration();
            let next = self.next_phase();
            self.enter_phase(next);
            self.phase_time = overflow;
        }

        self.stage.phase_progress = if self.phase_duration() > 0.0 {
            clamp01(self.phase_time / self.phase_duration())
        } else {
            1.0
        };

        self.animate(dt);
        self.particles.update(dt);
    }

    fn phase_duration(&self) -> f32 {
        match self.stage.phase {
            CasePhase::Intro => INTRO_SECONDS,
            CasePhase::Charge => CHARGE_SECONDS,
            CasePhase::Burst => BURST_SECONDS,
            CasePhase::Spin => SPIN_SECONDS,
            CasePhase::Reveal => REVEAL_SECONDS,
            CasePhase::Hold => self.hold_duration(),
            // Measured from the landing rather than the end of the spin, so the
            // winning slot stays up for the same beat however long it spun.
            CasePhase::Wheel => self
                .config
                .wheel
                .as_ref()
                .map(|wheel| wheel.spin_seconds * WHEEL_SETTLE_AT + RESULT_HOLD_FLOOR_SECONDS)
                .unwrap_or(0.0),
            CasePhase::Outro => OUTRO_SECONDS,
            CasePhase::Done => f32::INFINITY,
        }
    }

    fn hold_duration(&self) -> f32 {
        if self.wheel_pending {
            return PRE_WHEEL_HOLD_SECONDS;
        }
        let extended = if self.config.extended_result_hold {
            EXTENDED_HOLD_BONUS
        } else {
            0.0
        };
        // Rarer drops earn a little extra screen time, and a louder celebration
        // needs room to play out. Never less than the floor, though: the prize
        // has to be readable long enough to act on.
        ((BASE_HOLD_SECONDS + extended + self.intensity() * 1.5) * self.celebration.hold_scale())
            .max(RESULT_HOLD_FLOOR_SECONDS)
    }

    fn next_phase(&self) -> CasePhase {
        match self.stage.phase {
            CasePhase::Intro => CasePhase::Charge,
            CasePhase::Charge => CasePhase::Burst,
            CasePhase::Burst => CasePhase::Spin,
            CasePhase::Spin => CasePhase::Reveal,
            CasePhase::Reveal => CasePhase::Hold,
            CasePhase::Hold if self.wheel_pending => CasePhase::Wheel,
            CasePhase::Hold | CasePhase::Wheel => CasePhase::Outro,
            CasePhase::Outro | CasePhase::Done => CasePhase::Done,
        }
    }

    /// One-shot work when a phase begins: bursts, flashes, bookkeeping.
    fn enter_phase(&mut self, phase: CasePhase) {
        self.stage.phase = phase;
        self.stage.phase_progress = 0.0;
        let tier_color = self.tier_color();

        match phase {
            CasePhase::Intro => {
                self.particles.spawn_dust(
                    &mut self.rng,
                    90,
                    V3::new(760.0, 420.0, 460.0),
                    DUST_COLOR,
                );
            }
            CasePhase::Charge => {
                // The case landing punches a ring outward.
                self.particles.push_shockwave(Shockwave {
                    center: V3::ZERO,
                    radius: 40.0,
                    speed: 1500.0,
                    thickness: 26.0,
                    color: AppColor::from_rgb(210, 232, 255),
                    life: 0.55,
                    max_life: 0.55,
                    tilt: 0.0,
                    spin: 0.0,
                });
                self.particles.burst_sparks(
                    &mut self.rng,
                    V3::ZERO,
                    46,
                    620.0,
                    0.5,
                    AppColor::from_rgb(190, 226, 255),
                    0.5,
                );
                self.stage.flash = 0.24;
                self.stage.flash_color = AppColor::from_rgb(190, 224, 255);
            }
            CasePhase::Burst => {
                self.particles
                    .burst_shards(&mut self.rng, V3::ZERO, 44, 700.0, CASE_SHELL_COLOR);
                self.particles.burst_sparks(
                    &mut self.rng,
                    V3::ZERO,
                    150,
                    1150.0,
                    0.6,
                    SEAM_COLOR,
                    0.85,
                );
                for index in 0..3 {
                    let delay = index as f32;
                    self.particles.push_shockwave(Shockwave {
                        center: V3::ZERO,
                        radius: 30.0 + delay * 50.0,
                        speed: 2100.0 - delay * 420.0,
                        thickness: 30.0 - delay * 6.0,
                        color: SEAM_COLOR,
                        life: 0.75,
                        max_life: 0.75,
                        tilt: delay * 0.35,
                        spin: delay * 0.9,
                    });
                }
                self.stage.flash = 0.95;
                self.stage.flash_color = AppColor::from_rgb(255, 252, 236);
                self.reel_start = 0.0;
            }
            CasePhase::Spin => {
                self.ticks_crossed = 0;
            }
            CasePhase::Reveal => {
                let scene = self.celebration_scene(PRIZE_ORIGIN);
                let flash = self
                    .celebration
                    .fire(&mut self.particles, &mut self.rng, &scene, 1.0);
                self.particles
                    .spawn_embers(&mut self.rng, PRIZE_ORIGIN, 40, 260.0, tier_color);
                self.stage.flash = flash.strength;
                self.stage.flash_color = flash.color;
                self.beat_timer = 0.0;
                self.beats = 0;
            }
            CasePhase::Wheel => {
                self.stage.flash = 0.35;
                self.stage.flash_color = self.tier_color();
                self.beat_timer = 0.0;
                self.beats = 0;
            }
            CasePhase::Outro => {
                self.rays_at_outro = self.stage.rays;
                self.result_at_outro = self.stage.result_reveal.max(self.stage.winner_pop);
            }
            CasePhase::Hold | CasePhase::Done => {}
        }
    }

    /// Everything a celebration recipe needs about this drop.
    fn celebration_scene(&self, origin: V3) -> CelebrationScene {
        CelebrationScene {
            origin,
            tier_color: self.tier_color(),
            palette: self.confetti_palette(),
            intensity: self.intensity(),
        }
    }

    /// Where the wheel's winning slot ends up, for the landing celebration.
    fn wheel_prize_origin(&self) -> V3 {
        let prizes = self
            .config
            .wheel
            .as_ref()
            .map(|wheel| wheel.prizes.len())
            .unwrap_or(0);
        wheel_landing_position(prizes)
    }

    /// Ticks the celebration's continuing beats. `force` scales each beat, so
    /// the wheel's landing runs the same recipe louder than the first reveal.
    fn run_celebration_beats(&mut self, origin: V3, dt: f32, elapsed: f32, force: f32) {
        self.beat_timer += dt;
        let interval = self.celebration.beat_interval() / force.max(0.2);
        if self.beat_timer < interval {
            return;
        }
        self.beat_timer = 0.0;
        let scene = self.celebration_scene(origin);
        self.celebration.beat(
            &mut self.particles,
            &mut self.rng,
            &scene,
            self.beats,
            elapsed,
        );
        self.beats = self.beats.wrapping_add(1);
    }

    /// The second reveal: the wheel settling on a prize. Runs the tier's recipe
    /// at a higher force than the first reveal, because this only happens on the
    /// top tier and should land as the biggest moment of the sequence.
    fn celebrate_wheel_landing(&mut self) {
        if self.wheel_celebrated {
            return;
        }
        self.wheel_celebrated = true;
        let origin = self.wheel_prize_origin();
        let scene = self.celebration_scene(origin);
        let flash = self.celebration.fire(
            &mut self.particles,
            &mut self.rng,
            &scene,
            WHEEL_LANDING_FORCE,
        );
        self.stage.flash = flash.strength;
        self.stage.flash_color = flash.color;
        // A second recipe fired at the stage centre as well, so the celebration
        // fills the screen rather than clustering on one panel.
        let centre = self.celebration_scene(WHEEL_CENTER + V3::new(0.0, 120.0, 0.0));
        self.celebration
            .fire(&mut self.particles, &mut self.rng, &centre, 1.0);
        self.beat_timer = 0.0;
        self.beats = 0;
    }

    fn confetti_palette(&self) -> Vec<AppColor> {
        let tier = self.tier_color();
        vec![
            tier,
            AppColor::from_rgb(255, 255, 255),
            AppColor::from_rgb(255, 226, 150),
            lighten(tier, 0.4),
        ]
    }

    /// Recomputes every stage value for the current time.
    fn animate(&mut self, dt: f32) {
        let phase = self.stage.phase;
        let progress = self.stage.phase_progress;
        let elapsed = self.stage.elapsed;
        let intensity = self.intensity();
        let tier_color = self.tier_color();

        // Flashes always decay, regardless of phase.
        self.stage.flash = (self.stage.flash - dt * 3.4).max(0.0);
        self.stage.ticker_flash = (self.stage.ticker_flash - dt * 7.0).max(0.0);

        self.stage.backdrop = match phase {
            CasePhase::Intro => smoothstep(progress) * 0.82,
            CasePhase::Outro => (1.0 - smoothstep(progress)) * 0.82,
            CasePhase::Done => 0.0,
            _ => 0.82,
        };
        self.stage.global_fade = match phase {
            CasePhase::Outro => 1.0 - smootherstep(progress),
            CasePhase::Done => 0.0,
            _ => 1.0,
        };

        self.animate_case(phase, progress, elapsed);
        self.animate_reel(phase, progress, dt);
        self.animate_result(phase, progress, elapsed, intensity);
        self.animate_wheel(phase, progress, elapsed);
        self.animate_camera(phase, progress, elapsed, intensity);
        self.emit_ambient(phase, progress, dt, elapsed, intensity, tier_color);
    }

    fn animate_case(&mut self, phase: CasePhase, progress: f32, elapsed: f32) {
        match phase {
            CasePhase::Intro => {
                let eased = ease_out_back(progress, 1.25);
                self.stage.case_visible = true;
                // Comes in from deep behind the screen plane.
                self.stage.case_position = V3::new(0.0, 0.0, lerp(-2400.0, 0.0, eased));
                let spin = (1.0 - progress).powi(2);
                self.stage.case_rotation =
                    V3::new(spin * 3.4 + 0.16, -progress * 9.0 - spin * 6.0, spin * 1.6);
                self.stage.case_scale = lerp(0.35, 1.0, ease_out_cubic(progress));
                self.stage.case_opacity = smoothstep(progress * 3.0);
                self.stage.case_seam_glow = progress * 0.2;
                self.stage.case_lid_angle = 0.0;
                self.stage.case_pillar = 0.0;
            }
            CasePhase::Charge => {
                self.stage.case_visible = true;
                // Bob, plus a rattle that grows toward the crack.
                let rattle = progress.powi(3) * 9.0;
                let hover = (elapsed * 2.4).sin() * 10.0;
                self.stage.case_position = V3::new(
                    noise(elapsed * 47.0) * rattle,
                    hover + noise(elapsed * 53.0 + 11.0) * rattle,
                    noise(elapsed * 41.0 + 23.0) * rattle * 0.5,
                );
                self.stage.case_rotation = V3::new(
                    0.16 + (elapsed * 1.7).sin() * 0.05,
                    -elapsed * 1.25,
                    (elapsed * 2.1).sin() * 0.04,
                );
                self.stage.case_scale = 1.0 + progress * 0.06;
                self.stage.case_opacity = 1.0;
                // Heartbeat on top of a rising floor.
                let beat = ((elapsed * (7.0 + progress * 12.0)).sin() * 0.5 + 0.5).powi(2);
                self.stage.case_seam_glow = clamp01(0.2 + progress * 0.7 + beat * progress * 0.35);
                self.stage.case_lid_angle = progress.powi(4) * 0.12;
                self.stage.case_pillar = 0.0;
            }
            CasePhase::Burst => {
                self.stage.case_visible = true;
                let eased = ease_out_quint(progress);
                self.stage.case_position = V3::new(0.0, eased * 60.0, eased * 140.0);
                self.stage.case_rotation = V3::new(0.16 - eased * 0.5, -elapsed * 1.25, 0.0);
                self.stage.case_scale = 1.0 + eased * 0.9;
                self.stage.case_opacity = 1.0 - smoothstep(progress * 1.15);
                self.stage.case_seam_glow = 1.0;
                self.stage.case_lid_angle = eased * 2.1;
                self.stage.case_pillar = ease::pulse(progress, 5.0);
            }
            _ => {
                self.stage.case_visible = false;
                self.stage.case_pillar = 0.0;
            }
        }
    }

    fn animate_reel(&mut self, phase: CasePhase, progress: f32, dt: f32) {
        let previous_scroll = self.stage.reel_scroll;
        match phase {
            CasePhase::Burst => {
                self.stage.reel_visible = true;
                self.stage.reel_assemble = smoothstep(progress);
                self.stage.reel_opacity = self.stage.reel_assemble;
                self.stage.reel_scroll = 0.0;
            }
            CasePhase::Spin => {
                self.stage.reel_visible = true;
                self.stage.reel_assemble = 1.0;
                self.stage.reel_opacity = 1.0;
                self.stage.reel_scroll = self.reel_start
                    + (self.reel_target - self.reel_start) * REEL_CURVE.eval(progress);
            }
            CasePhase::Reveal | CasePhase::Hold => {
                self.stage.reel_visible = true;
                self.stage.reel_assemble = 1.0;
                self.stage.reel_scroll = self.reel_target;
                // The strip dissolves as the winner takes over.
                self.stage.reel_opacity = if phase == CasePhase::Reveal {
                    1.0 - smoothstep(progress * 1.4)
                } else {
                    0.0
                };
            }
            CasePhase::Wheel | CasePhase::Outro => {
                self.stage.reel_visible = false;
                self.stage.reel_opacity = 0.0;
                self.stage.reel_scroll = self.reel_target;
            }
            _ => {
                self.stage.reel_visible = false;
                self.stage.reel_opacity = 0.0;
            }
        }

        self.stage.reel_velocity = if dt > 0.0 {
            (self.stage.reel_scroll - previous_scroll) / dt
        } else {
            0.0
        };

        // Fire a tick every time a card boundary passes the centre line.
        if phase == CasePhase::Spin {
            let crossed = (self.stage.reel_scroll / CARD_PITCH).floor() as i64;
            if crossed > self.ticks_crossed {
                let ticks = (crossed - self.ticks_crossed).min(4);
                self.ticks_crossed = crossed;
                self.stage.ticker_flash = 1.0;
                let speed_factor = clamp01(self.stage.reel_velocity.abs() / 4000.0);
                for _ in 0..ticks {
                    // Slow ticks get fatter sparks, which reads as weight.
                    self.particles.burst_sparks(
                        &mut self.rng,
                        V3::new(0.0, CARD_HEIGHT * 0.5 + 18.0, 40.0),
                        (4.0 + (1.0 - speed_factor) * 10.0) as usize,
                        180.0 + speed_factor * 320.0,
                        0.7,
                        TICKER_COLOR,
                        0.22 + (1.0 - speed_factor) * 0.3,
                    );
                }
            }
        }
    }

    fn animate_result(&mut self, phase: CasePhase, progress: f32, elapsed: f32, intensity: f32) {
        match phase {
            CasePhase::Reveal => {
                self.stage.winner_pop = ease_out_elastic(clamp01(progress * 1.5));
                self.stage.winner_lift = ease_out_cubic(clamp01(progress * 1.3));
                self.stage.debris_fall = ease_in_out_fall(progress);
                self.stage.rays = smoothstep(progress * 1.6) * (0.55 + intensity * 0.45);
                self.stage.result_reveal = smoothstep((progress - 0.35) / 0.65);
            }
            CasePhase::Hold => {
                self.stage.winner_pop = 1.0;
                self.stage.winner_lift = 1.0;
                self.stage.debris_fall = 1.0;
                // Slow breathing on the rays so the hold isn't static.
                let breath = (elapsed * 1.6).sin() * 0.5 + 0.5;
                self.stage.rays =
                    (0.55 + intensity * 0.45) * (0.78 + breath * 0.22) * (1.0 - progress * 0.25);
                self.stage.result_reveal = 1.0;
            }
            CasePhase::Wheel => {
                // The card has had its moment. It clears out on a fixed clock
                // rather than a share of this phase, whose length follows the
                // configured spin time, so the wheel never has to share the
                // stage with a headline no matter how long the spin runs.
                let exit = clamp01(self.phase_time / WHEEL_HANDOFF_SECONDS);
                self.stage.winner_pop = 1.0 - ease::ease_in_cubic(exit);
                self.stage.winner_lift = 1.0;
                self.stage.rays = (0.35 + intensity * 0.3) * (1.0 - smoothstep(exit));
                self.stage.result_reveal = 1.0 - smoothstep(exit);
            }
            CasePhase::Outro => {
                // Fades from whatever the outro inherited. A wheel run leaves
                // nothing behind, and reading a level captured on entry avoids
                // the per-frame decay that scaling `stage` in place would give.
                let out = 1.0 - ease::ease_in_cubic(progress);
                self.stage.winner_pop = self.result_at_outro * out;
                self.stage.rays = self.rays_at_outro * (1.0 - progress);
                self.stage.result_reveal =
                    self.result_at_outro * (1.0 - smoothstep(progress * 1.5));
            }
            _ => {
                self.stage.winner_pop = 0.0;
                self.stage.winner_lift = 0.0;
                self.stage.debris_fall = 0.0;
                self.stage.rays = 0.0;
                self.stage.result_reveal = 0.0;
            }
        }
    }

    fn animate_wheel(&mut self, phase: CasePhase, progress: f32, elapsed: f32) {
        let Some(wheel) = self.config.wheel.as_ref() else {
            self.stage.wheel_visible = false;
            return;
        };
        if phase != CasePhase::Wheel {
            if phase == CasePhase::Outro && self.wheel_pending {
                // Keep it on screen while everything fades.
                self.stage.wheel_scale = 1.0 - smoothstep(progress);
            } else {
                self.stage.wheel_visible = false;
                self.stage.wheel_scale = 0.0;
            }
            return;
        }

        let prize_count = wheel.prizes.len().max(1);
        let winner = self.wheel_prize_index.unwrap_or(0);
        let spin_fraction = clamp01(self.phase_time / wheel.spin_seconds);
        // Five full turns before landing, so the deceleration is legible.
        let slot_angle = std::f32::consts::TAU / prize_count as f32;
        let target = 5.0 * std::f32::consts::TAU + winner as f32 * slot_angle;

        self.stage.wheel_visible = true;
        // Waits for the winning card to clear before growing in.
        self.stage.wheel_scale = ease_out_back(
            clamp01((self.phase_time - WHEEL_HANDOFF_SECONDS) / 0.5),
            1.1,
        );
        self.stage.wheel_angle = target * WHEEL_CURVE.eval(spin_fraction);
        let settled = clamp01((self.phase_time - wheel.spin_seconds * WHEEL_SETTLE_AT) / 0.45);
        self.stage.wheel_glow = 0.35 + settled * 0.65 * (0.85 + (elapsed * 5.0).sin() * 0.15);
        self.stage.wheel_reveal = smoothstep(settled);
        if settled > 0.0 {
            self.celebrate_wheel_landing();
        }
    }

    fn animate_camera(&mut self, phase: CasePhase, progress: f32, elapsed: f32, intensity: f32) {
        let mut rig = CameraRig::default();

        let (distance, height, shake) = match phase {
            CasePhase::Intro => (lerp(2050.0, 1500.0, smootherstep(progress)), 40.0, 0.0),
            CasePhase::Charge => (
                lerp(1500.0, 1380.0, smoothstep(progress)),
                40.0,
                progress.powi(3) * 4.0,
            ),
            CasePhase::Burst => (lerp(1380.0, 1620.0, ease_out_quint(progress)), 30.0, 16.0),
            CasePhase::Spin => {
                // Shake tracks reel speed, so it fades as the strip settles.
                let speed = clamp01(self.stage.reel_velocity.abs() / 5200.0);
                (lerp(1660.0, 1500.0, progress), 10.0, speed * 9.0)
            }
            CasePhase::Reveal => (
                lerp(1500.0, 1180.0, ease_out_cubic(progress)),
                10.0,
                (1.0 - progress).powi(2) * (10.0 + intensity * 14.0),
            ),
            CasePhase::Hold => (
                1180.0 + (elapsed * 0.7).sin() * 24.0,
                10.0 + (elapsed * 0.9).cos() * 8.0,
                0.0,
            ),
            CasePhase::Wheel => (
                lerp(1180.0, 1560.0, smootherstep(progress.min(0.4) / 0.4)),
                40.0,
                0.0,
            ),
            CasePhase::Outro => (lerp(1180.0, 1020.0, progress), 10.0, 0.0),
            CasePhase::Done => (1450.0, 40.0, 0.0),
        };

        rig.eye = V3::new(0.0, height, distance);
        rig.target = V3::new(0.0, height * 0.35, 0.0);

        if shake > 0.0 {
            let offset = V3::new(
                noise(elapsed * 61.0) * shake,
                noise(elapsed * 67.0 + 31.0) * shake,
                noise(elapsed * 43.0 + 71.0) * shake * 0.4,
            );
            rig.eye += offset;
            rig.target += offset * 0.35;
            rig.roll = noise(elapsed * 37.0 + 13.0) * shake * 0.0016;
        }

        // A gentle drift keeps the framing alive even when nothing is moving.
        let drift = V3::new(
            (elapsed * 0.43).sin() * 12.0,
            (elapsed * 0.31).cos() * 8.0,
            0.0,
        );
        rig.eye += drift;

        self.stage.camera = rig;
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_ambient(
        &mut self,
        phase: CasePhase,
        progress: f32,
        dt: f32,
        elapsed: f32,
        intensity: f32,
        tier_color: AppColor,
    ) {
        // Ambient dust runs for the whole sequence except the outro.
        self.dust_timer += dt;
        if self.dust_timer > 0.12 && !matches!(phase, CasePhase::Outro | CasePhase::Done) {
            self.dust_timer = 0.0;
            self.particles
                .spawn_dust(&mut self.rng, 3, V3::new(780.0, 430.0, 470.0), DUST_COLOR);
        }

        match phase {
            CasePhase::Charge => {
                // Energy spiralling into the case, thickening as it charges.
                let case_position = self.stage.case_position;
                self.particles.attractor = case_position;
                self.streak_timer += dt;
                if self.streak_timer > 0.045 {
                    self.streak_timer = 0.0;
                    let count = 2 + (progress * 5.0) as usize;
                    self.particles.spawn_infalling(
                        &mut self.rng,
                        case_position,
                        count,
                        460.0,
                        SEAM_COLOR,
                    );
                }
            }
            CasePhase::Spin => {
                let speed = clamp01(self.stage.reel_velocity.abs() / 5200.0);
                self.streak_timer += dt;
                if self.streak_timer > 0.02 && speed > 0.06 {
                    self.streak_timer = 0.0;
                    self.particles.spawn_streaks(
                        &mut self.rng,
                        (1.0 + speed * 5.0) as usize,
                        V3::new(900.0, 300.0, 200.0),
                        900.0 + speed * 3200.0,
                        STREAK_COLOR,
                    );
                }
            }
            CasePhase::Hold => {
                // Embers orbiting the prize, denser for rarer tiers.
                self.ember_timer += dt;
                if self.ember_timer > 0.09 {
                    self.ember_timer = 0.0;
                    self.particles.spawn_embers(
                        &mut self.rng,
                        V3::new(0.0, -40.0, 60.0),
                        1 + (intensity * 3.0) as usize,
                        280.0,
                        tier_color,
                    );
                }
                // The tier's own celebration keeps going while the prize is up.
                self.run_celebration_beats(PRIZE_ORIGIN, dt, elapsed, 1.0);
            }
            CasePhase::Wheel => {
                self.ember_timer += dt;
                if self.ember_timer > 0.05 {
                    self.ember_timer = 0.0;
                    self.particles
                        .spawn_embers(&mut self.rng, WHEEL_CENTER, 2, 420.0, tier_color);
                }
                // Once it lands, the wheel gets the same treatment turned up.
                if self.stage.wheel_reveal > 0.05 {
                    self.run_celebration_beats(self.wheel_prize_origin(), dt, elapsed, 1.3);
                }
            }
            _ => {}
        }
    }
}

/// Non-winning cards hang for a beat, then drop away fast.
fn ease_in_out_fall(progress: f32) -> f32 {
    let t = clamp01((progress - 0.12) / 0.88);
    t * t
}

/// Deterministic value noise in -1..1. Cheap and good enough for shake.
fn noise(t: f32) -> f32 {
    let base = t.floor();
    let frac = t - base;
    let smooth = frac * frac * (3.0 - 2.0 * frac);
    let a = hash01(base);
    let b = hash01(base + 1.0);
    (a + (b - a) * smooth) * 2.0 - 1.0
}

fn hash01(value: f32) -> f32 {
    let mut bits = (value * 127.1 + 311.7).to_bits();
    bits ^= bits >> 15;
    bits = bits.wrapping_mul(0x2545_F491);
    bits ^= bits >> 13;
    (bits & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

fn lighten(color: AppColor, amount: f32) -> AppColor {
    let mix = |channel: u8| {
        let value = channel as f32 / 255.0;
        (((value + (1.0 - value) * amount) * 255.0).round() as u8).min(255)
    };
    AppColor::from_argb(color.a, mix(color.r), mix(color.g), mix(color.b))
}

fn default_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x5EED)
        // Mix so consecutive openings do not start from adjacent seeds.
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        | 1
}

const DUST_COLOR: AppColor = AppColor::from_rgb(150, 178, 210);
const SEAM_COLOR: AppColor = AppColor::from_rgb(255, 214, 128);
const CASE_SHELL_COLOR: AppColor = AppColor::from_rgb(118, 132, 152);
const TICKER_COLOR: AppColor = AppColor::from_rgb(255, 240, 205);
const STREAK_COLOR: AppColor = AppColor::from_rgb(150, 190, 240);

#[cfg(test)]
mod tests {
    use super::*;

    fn run(session: &mut CaseSession, seconds: f32) {
        let step: f32 = 1.0 / 120.0;
        let mut remaining = seconds;
        while remaining > 0.0 {
            session.update(step.min(remaining));
            remaining -= step;
        }
    }

    fn session_for_tier(tier: &str) -> CaseSession {
        CaseSession::new(
            CaseConfig::default(),
            CaseRequest {
                viewer: "Tester".to_string(),
                forced_tier: Some(tier.to_string()),
                seed: Some(1234),
                streak: 1,
                ..CaseRequest::default()
            },
        )
    }

    #[test]
    fn the_sequence_walks_every_phase_and_finishes() {
        let mut session = session_for_tier("mil_spec");
        let mut seen = Vec::new();
        for _ in 0..6000 {
            session.update(1.0 / 60.0);
            if seen.last() != Some(&session.stage.phase) {
                seen.push(session.stage.phase);
            }
            if session.is_finished() {
                break;
            }
        }
        assert!(session.is_finished(), "sequence should terminate");
        assert_eq!(
            seen,
            vec![
                CasePhase::Intro,
                CasePhase::Charge,
                CasePhase::Burst,
                CasePhase::Spin,
                CasePhase::Reveal,
                CasePhase::Hold,
                CasePhase::Outro,
                CasePhase::Done,
            ]
        );
    }

    #[test]
    fn the_top_tier_detours_through_the_wheel() {
        let mut session = session_for_tier("rare_special");
        let mut saw_wheel = false;
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            saw_wheel |= session.stage.phase == CasePhase::Wheel;
            if session.is_finished() {
                break;
            }
        }
        assert!(session.is_finished());
        assert!(saw_wheel, "rare special should spin the legendary wheel");
        assert!(session.result().wheel_prize.is_some());
    }

    #[test]
    fn every_drop_stays_up_for_five_seconds_after_it_is_shown() {
        for tier in &CaseConfig::default().tiers {
            let mut session = session_for_tier(&tier.id);
            // Time from the prize being legible to the sequence starting to fade.
            let mut shown_at = None;
            let mut fade_at = None;
            let mut clock = 0.0f32;
            while !session.is_finished() && clock < 60.0 {
                session.update(1.0 / 120.0);
                clock += 1.0 / 120.0;
                let legible = match session.stage.phase {
                    // The wheel run's prize is the slot the pointer lands on.
                    CasePhase::Wheel => session.stage.wheel_reveal > 0.0,
                    CasePhase::Hold => !session.wheel_pending,
                    _ => false,
                };
                if legible && shown_at.is_none() {
                    shown_at = Some(clock);
                }
                if shown_at.is_some()
                    && fade_at.is_none()
                    && session.stage.phase == CasePhase::Outro
                {
                    fade_at = Some(clock);
                }
            }
            let shown = shown_at.expect("prize was never shown");
            let fade = fade_at.expect("sequence never faded");
            assert!(
                fade - shown >= RESULT_HOLD_FLOOR_SECONDS - 0.05,
                "{} held its prize for only {:.2}s",
                tier.name,
                fade - shown
            );
        }
    }

    #[test]
    fn every_tier_of_the_default_case_gets_its_own_celebration() {
        let config = CaseConfig::default();
        let mut seen = Vec::new();
        for tier in &config.tiers {
            let session = session_for_tier(&tier.id);
            let celebration = session.celebration();
            assert!(
                !seen.contains(&celebration),
                "{} reuses the {} celebration",
                tier.name,
                celebration.label()
            );
            seen.push(celebration);
        }
        assert_eq!(seen.len(), config.tiers.len());
    }

    #[test]
    fn the_wheel_landing_fires_its_own_celebration() {
        let mut session = session_for_tier("rare_special");
        let mut flash_at_landing = 0.0f32;
        let mut particles_at_landing = 0usize;
        let mut landing_angle_left = f32::INFINITY;
        let mut was_settled = false;
        let mut final_angle = 0.0f32;
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            if session.stage.phase == CasePhase::Wheel {
                final_angle = session.stage.wheel_angle;
            }
            let settled = session.stage.wheel_reveal > 0.0;
            if settled && !was_settled {
                // The frame the wheel stops: the recipe should have just fired.
                flash_at_landing = session.stage.flash;
                particles_at_landing = session.particles.live_count();
                landing_angle_left = session.stage.wheel_angle;
            }
            was_settled = settled;
            if session.is_finished() {
                break;
            }
        }
        assert!(was_settled, "the wheel never settled");
        // The wheel should be all but stopped when the celebration goes off:
        // under a tenth of a slot left to creep through.
        let prizes = session.config().wheel.as_ref().unwrap().prizes.len();
        let slot_angle = std::f32::consts::TAU / prizes as f32;
        let remaining = (final_angle - landing_angle_left).abs();
        assert!(
            remaining < slot_angle * 0.1,
            "the celebration fired with {remaining:.3} rad still to turn, \
             more than a tenth of a {slot_angle:.3} rad slot"
        );
        assert!(
            flash_at_landing > 0.5,
            "the landing barely flashed: {flash_at_landing}"
        );
        assert!(
            particles_at_landing > 400,
            "the landing only emitted {particles_at_landing} particles"
        );
    }

    #[test]
    fn the_card_is_gone_before_the_wheel_takes_the_stage() {
        let mut session = session_for_tier("rare_special");
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            let stage = &session.stage;
            if stage.wheel_scale > 0.05 {
                assert!(
                    stage.winner_pop < 0.2 && stage.result_reveal < 0.2,
                    "card (pop {:.2}, text {:.2}) still on screen with the wheel at {:.2}",
                    stage.winner_pop,
                    stage.result_reveal,
                    stage.wheel_scale
                );
            }
            if session.is_finished() {
                break;
            }
        }
    }

    #[test]
    fn lower_tiers_skip_the_wheel() {
        let mut session = session_for_tier("covert");
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            assert_ne!(session.stage.phase, CasePhase::Wheel);
            if session.is_finished() {
                break;
            }
        }
        assert!(session.result().wheel_prize.is_none());
    }

    #[test]
    fn the_reel_lands_the_winning_card_near_the_ticker() {
        let mut session = session_for_tier("classified");
        run(
            &mut session,
            INTRO_SECONDS + CHARGE_SECONDS + BURST_SECONDS + SPIN_SECONDS + 0.1,
        );
        let landed = session.stage.reel_scroll;
        let winner_center = session.winner_slot() as f32 * CARD_PITCH;
        let offset = (landed - winner_center).abs();
        assert!(
            offset < CARD_WIDTH * 0.5,
            "winner drifted {offset:.1} units off the ticker"
        );
    }

    #[test]
    fn the_reel_only_ever_moves_forward() {
        let mut session = session_for_tier("mil_spec");
        run(&mut session, INTRO_SECONDS + CHARGE_SECONDS + BURST_SECONDS);
        let mut previous = session.stage.reel_scroll;
        for _ in 0..(SPIN_SECONDS * 120.0) as usize {
            session.update(1.0 / 120.0);
            assert!(
                session.stage.reel_scroll >= previous - 1e-3,
                "reel went backwards"
            );
            previous = session.stage.reel_scroll;
        }
    }

    #[test]
    fn a_long_stalled_frame_cannot_skip_the_reel_past_its_landing() {
        let mut session = session_for_tier("covert");
        for _ in 0..400 {
            // Ten-second frames, as if the machine had been asleep.
            session.update(10.0);
            assert!(session.stage.reel_scroll <= session.reel_target + 1e-3);
            if session.is_finished() {
                break;
            }
        }
        assert!(session.is_finished());
    }

    #[test]
    fn the_same_seed_produces_the_same_strip_and_drop() {
        let build = || {
            CaseSession::new(
                CaseConfig::default(),
                CaseRequest {
                    viewer: "A".to_string(),
                    seed: Some(0xFEED),
                    ..CaseRequest::default()
                },
            )
        };
        let left = build();
        let right = build();
        assert_eq!(left.outcome(), right.outcome());
        assert_eq!(left.result(), right.result());
        let strips_match = left
            .reel
            .iter()
            .zip(right.reel.iter())
            .all(|(a, b)| a.tier_index == b.tier_index && a.reward_index == b.reward_index);
        assert!(strips_match, "seeded strips should match");
    }

    #[test]
    fn the_winning_slot_holds_the_rolled_prize() {
        let session = session_for_tier("covert");
        let slot = session.reel[session.winner_slot()];
        assert_eq!(slot.tier_index, session.outcome().tier_index);
        assert_eq!(slot.reward_index, session.outcome().reward_index);
    }

    #[test]
    fn particles_never_exceed_the_pool_over_a_full_run() {
        let mut session = session_for_tier("rare_special");
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            assert!(session.particles.particles.len() <= crate::particles::MAX_PARTICLES);
            assert!(session.particles.shockwaves.len() <= crate::particles::MAX_SHOCKWAVES);
            if session.is_finished() {
                break;
            }
        }
    }

    #[test]
    fn stage_values_stay_inside_their_documented_ranges() {
        let mut session = session_for_tier("rare_special");
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            let stage = &session.stage;
            for (name, value) in [
                ("phase_progress", stage.phase_progress),
                ("backdrop", stage.backdrop / 0.82),
                ("flash", stage.flash),
                ("global_fade", stage.global_fade),
                ("case_opacity", stage.case_opacity),
                ("case_seam_glow", stage.case_seam_glow),
                ("case_pillar", stage.case_pillar),
                ("reel_opacity", stage.reel_opacity),
                ("reel_assemble", stage.reel_assemble),
                ("ticker_flash", stage.ticker_flash),
                ("debris_fall", stage.debris_fall),
                ("rays", stage.rays),
                ("result_reveal", stage.result_reveal),
                ("wheel_reveal", stage.wheel_reveal),
            ] {
                assert!(
                    (-0.001..=1.001).contains(&value),
                    "{name} left 0..1 with {value}"
                );
            }
            if session.is_finished() {
                break;
            }
        }
    }

    #[test]
    fn the_result_settles_before_the_sequence_ends() {
        let mut session = session_for_tier("restricted");
        let mut settled_at = None;
        let mut ticks = 0;
        while !session.is_finished() && ticks < 8000 {
            session.update(1.0 / 60.0);
            if settled_at.is_none() && session.result_is_settled() {
                settled_at = Some(session.stage.elapsed);
            }
            ticks += 1;
        }
        let settled = settled_at.expect("result should settle");
        assert!(settled < session.stage.elapsed - 1.0);
    }

    #[test]
    fn text_lines_reflect_the_request_and_the_roll() {
        let mut session = CaseSession::new(
            CaseConfig::default(),
            CaseRequest {
                viewer: "zedandgone".to_string(),
                forced_tier: Some("covert".to_string()),
                seed: Some(5),
                streak: 4,
                ..CaseRequest::default()
            },
        );
        assert_eq!(session.header_line(), "ZEDANDGONE OPENED  ::  STREAK X4");
        assert!(session.tier_line().contains("COVERT"));
        assert!(session.tier_line().contains("0.64%"));
        run(&mut session, 0.1);
        assert_eq!(session.status_line(), "ROLLING THE CASE");
    }

    #[test]
    fn a_blank_viewer_name_still_reads() {
        let session = CaseSession::new(CaseConfig::default(), CaseRequest::for_viewer("   "));
        assert_eq!(session.viewer_name(), "VIEWER");
    }

    #[test]
    fn a_case_without_a_wheel_still_completes_on_the_top_tier() {
        let mut config = CaseConfig::default();
        config.wheel = None;
        let mut session = CaseSession::new(
            config,
            CaseRequest {
                forced_tier: Some("rare_special".to_string()),
                seed: Some(3),
                ..CaseRequest::default()
            },
        );
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            assert_ne!(session.stage.phase, CasePhase::Wheel);
            if session.is_finished() {
                break;
            }
        }
        assert!(session.is_finished());
    }

    #[test]
    fn a_single_tier_case_does_not_divide_by_zero() {
        let text = r##"{"profiles":[{"id":"a","name":"One","rarities":[
            {"id":"only","name":"Only","odds":100,"color":"#ffffff","rewards":[{"name":"Prize"}]}
        ]}]}"##;
        let config = CaseConfig::from_rewards_json(text, None).expect("config should parse");
        let mut session = CaseSession::new(config, CaseRequest::for_viewer("x"));
        run(&mut session, 2.0);
        assert!(session.intensity().is_finite());
    }
}
