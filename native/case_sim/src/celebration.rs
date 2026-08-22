//! Reveal celebrations, one per step up the rarity ladder.
//!
//! Each tier gets a different *shape* of celebration rather than the same burst
//! with the particle count turned up, so a viewer can tell what someone hit from
//! the corner of their eye before any text is readable. They do escalate as
//! well: more emitters, longer lives, brighter flashes.
//!
//! The same recipes drive the legendary wheel's landing, run at a higher force,
//! which keeps the second reveal recognisably part of the same show.

use core_types::AppColor;

use crate::math::V3;
use crate::particles::{ParticleSystem, Shockwave};
use crate::rng::Rng;

/// What a celebration needs to know about the drop it is dressing.
pub struct CelebrationScene {
    /// Where the prize sits in case space.
    pub origin: V3,
    pub tier_color: AppColor,
    /// Confetti colours, already mixed for this tier.
    pub palette: Vec<AppColor>,
    /// 0 for the floor tier, 1 for the top of the ladder.
    pub intensity: f32,
}

/// The screen flash a celebration asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flash {
    pub strength: f32,
    pub color: AppColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Celebration {
    /// Floor tier: one crisp ring and a short spark puff. Over in a beat.
    Clean,
    /// Counter-tilted rings with a column of embers climbing behind the prize.
    Plume,
    /// Confetti and speed lines snapping outward from the card.
    Streamers,
    /// The case cracking a second time: shards, hot sparks, a double flash.
    Shatter,
    /// Everything, and it keeps coming: side cannons, spiral embers, pulses.
    Jackpot,
}

impl Celebration {
    /// Picks the recipe for a tier's place on the ladder. Bands rather than an
    /// index, so a config with three or eight tiers still escalates sensibly.
    pub fn for_intensity(intensity: f32) -> Self {
        match intensity {
            level if level < 0.12 => Self::Clean,
            level if level < 0.35 => Self::Plume,
            level if level < 0.60 => Self::Streamers,
            level if level < 0.85 => Self::Shatter,
            _ => Self::Jackpot,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Plume => "plume",
            Self::Streamers => "streamers",
            Self::Shatter => "shatter",
            Self::Jackpot => "jackpot",
        }
    }

    /// How long the celebration wants the prize held, as a multiplier on the
    /// base hold. The loud ones need room to finish.
    pub fn hold_scale(&self) -> f32 {
        match self {
            Self::Clean => 0.85,
            Self::Plume => 0.95,
            Self::Streamers => 1.0,
            Self::Shatter => 1.1,
            Self::Jackpot => 1.2,
        }
    }

    /// Seconds between [`Celebration::beat`] calls.
    pub fn beat_interval(&self) -> f32 {
        match self {
            Self::Clean => 1.6,
            Self::Plume => 1.1,
            Self::Streamers => 0.85,
            Self::Shatter => 0.7,
            Self::Jackpot => 0.45,
        }
    }

    /// The one-shot burst at the moment of reveal. `force` scales the whole
    /// recipe: 1.0 for a normal drop, higher for the legendary wheel's landing.
    pub fn fire(
        &self,
        particles: &mut ParticleSystem,
        rng: &mut Rng,
        scene: &CelebrationScene,
        force: f32,
    ) -> Flash {
        let hot = lighten(scene.tier_color, 0.45);
        let count = |base: f32| (base * force).round().max(1.0) as usize;
        match self {
            Self::Clean => {
                ring(
                    particles,
                    scene.origin,
                    RingShape {
                        color: lighten(scene.tier_color, 0.25),
                        radius: 70.0,
                        speed: 1200.0 * force,
                        thickness: 14.0,
                        life: 0.55,
                        tilt: 0.0,
                        spin: 0.0,
                    },
                );
                particles.burst_sparks(
                    rng,
                    scene.origin,
                    count(54.0),
                    780.0 * force,
                    0.5,
                    lighten(scene.tier_color, 0.3),
                    0.55,
                );
                Flash {
                    strength: 0.30 * force,
                    color: lighten(scene.tier_color, 0.6),
                }
            }
            Self::Plume => {
                for (index, tilt) in [0.35f32, -0.35].into_iter().enumerate() {
                    ring(
                        particles,
                        scene.origin,
                        RingShape {
                            color: scene.tier_color,
                            radius: 80.0 + index as f32 * 40.0,
                            speed: (1350.0 - index as f32 * 200.0) * force,
                            thickness: 18.0,
                            life: 0.8,
                            tilt,
                            spin: if index == 0 { 1.1 } else { -1.1 },
                        },
                    );
                }
                particles.burst_sparks(
                    rng,
                    scene.origin,
                    count(90.0),
                    900.0 * force,
                    0.6,
                    hot,
                    0.7,
                );
                // The plume: a column climbing out from behind the prize.
                particles.fountain_embers(
                    rng,
                    scene.origin - V3::new(0.0, 150.0, 40.0),
                    V3::new(0.0, 1.0, 0.0),
                    count(46.0),
                    340.0,
                    0.22,
                    scene.tier_color,
                );
                Flash {
                    strength: 0.42 * force,
                    color: hot,
                }
            }
            Self::Streamers => {
                for index in 0..3 {
                    let delay = index as f32;
                    ring(
                        particles,
                        scene.origin,
                        RingShape {
                            color: scene.tier_color,
                            radius: 70.0 + delay * 45.0,
                            speed: (1600.0 - delay * 260.0) * force,
                            thickness: 20.0,
                            life: 0.9,
                            tilt: delay * 0.4,
                            spin: delay * 1.2,
                        },
                    );
                }
                particles.burst_sparks(
                    rng,
                    scene.origin,
                    count(120.0),
                    1000.0 * force,
                    0.65,
                    hot,
                    0.8,
                );
                particles.burst_confetti(
                    rng,
                    scene.origin,
                    count(90.0),
                    640.0 * force,
                    &scene.palette,
                );
                // Speed lines snapping away from the card, both directions.
                for aim in [V3::new(1.0, 0.15, 0.0), V3::new(-1.0, 0.15, 0.0)] {
                    particles.spawn_streaks(
                        rng,
                        count(14.0),
                        V3::new(760.0 * aim.x.signum(), 260.0, 180.0),
                        1500.0 * force,
                        lighten(scene.tier_color, 0.5),
                    );
                }
                Flash {
                    strength: 0.55 * force,
                    color: hot,
                }
            }
            Self::Shatter => {
                particles.burst_shards(
                    rng,
                    scene.origin,
                    count(56.0),
                    900.0 * force,
                    lighten(scene.tier_color, 0.15),
                );
                particles.burst_sparks(
                    rng,
                    scene.origin,
                    count(190.0),
                    1400.0 * force,
                    0.75,
                    hot,
                    0.9,
                );
                particles.burst_confetti(
                    rng,
                    scene.origin,
                    count(110.0),
                    700.0 * force,
                    &scene.palette,
                );
                // Two hard rings close together read as a crack, not a pulse.
                for index in 0..2 {
                    let delay = index as f32;
                    ring(
                        particles,
                        scene.origin,
                        RingShape {
                            color: lighten(scene.tier_color, 0.3),
                            radius: 50.0 + delay * 26.0,
                            speed: (2200.0 - delay * 320.0) * force,
                            thickness: 30.0 - delay * 8.0,
                            life: 0.7,
                            tilt: delay * 0.2,
                            spin: delay * 0.6,
                        },
                    );
                }
                particles.fountain_embers(
                    rng,
                    scene.origin,
                    V3::new(0.0, 1.0, 0.0),
                    count(50.0),
                    420.0,
                    0.5,
                    hot,
                );
                Flash {
                    strength: 0.78 * force,
                    color: AppColor::from_rgb(255, 246, 232),
                }
            }
            Self::Jackpot => {
                // Cannons firing in from both edges, aimed up and inward.
                for side in [-1.0f32, 1.0] {
                    particles.cannon_confetti(
                        rng,
                        scene.origin + V3::new(side * 900.0, -260.0, -60.0),
                        V3::new(-side * 0.72, 1.0, 0.1),
                        count(90.0),
                        1500.0,
                        0.24,
                        &scene.palette,
                    );
                }
                particles.burst_confetti(
                    rng,
                    scene.origin,
                    count(120.0),
                    780.0 * force,
                    &scene.palette,
                );
                particles.burst_sparks(
                    rng,
                    scene.origin,
                    count(240.0),
                    1650.0 * force,
                    0.8,
                    hot,
                    1.1,
                );
                particles.burst_shards(rng, scene.origin, count(40.0), 780.0, hot);
                // A staggered stack of rings, alternating tilt into a spiral.
                for index in 0..4 {
                    let delay = index as f32;
                    ring(
                        particles,
                        scene.origin,
                        RingShape {
                            color: if index % 2 == 0 {
                                scene.tier_color
                            } else {
                                hot
                            },
                            radius: 60.0 + delay * 44.0,
                            speed: (2000.0 - delay * 260.0) * force,
                            thickness: 26.0,
                            life: 1.05,
                            tilt: delay * 0.5 * if index % 2 == 0 { 1.0 } else { -1.0 },
                            spin: delay * 1.4,
                        },
                    );
                }
                particles.fountain_embers(
                    rng,
                    scene.origin - V3::new(0.0, 200.0, 0.0),
                    V3::new(0.0, 1.0, 0.0),
                    count(80.0),
                    480.0,
                    0.35,
                    hot,
                );
                Flash {
                    // A full-strength white-out; the stage contract caps here.
                    strength: (1.05 * force).min(1.0),
                    color: AppColor::from_rgb(255, 250, 226),
                }
            }
        }
    }

    /// One beat of the celebration continuing while the prize is held. `beat`
    /// counts up from zero so recipes can alternate sides.
    pub fn beat(
        &self,
        particles: &mut ParticleSystem,
        rng: &mut Rng,
        scene: &CelebrationScene,
        beat: u32,
        elapsed: f32,
    ) {
        let hot = lighten(scene.tier_color, 0.45);
        let side = if beat.is_multiple_of(2) { 1.0f32 } else { -1.0 };
        match self {
            Self::Clean => {
                particles.spawn_embers(rng, scene.origin, 2, 240.0, scene.tier_color);
            }
            Self::Plume => {
                particles.fountain_embers(
                    rng,
                    scene.origin - V3::new(0.0, 150.0, 40.0),
                    V3::new(0.0, 1.0, 0.0),
                    7,
                    300.0,
                    0.2,
                    scene.tier_color,
                );
            }
            Self::Streamers => {
                particles.cannon_confetti(
                    rng,
                    scene.origin + V3::new(side * 420.0, -240.0, -40.0),
                    V3::new(-side * 0.35, 1.0, 0.0),
                    16,
                    900.0,
                    0.3,
                    &scene.palette,
                );
                particles.spawn_embers(rng, scene.origin, 3, 280.0, scene.tier_color);
            }
            Self::Shatter => {
                particles.burst_sparks(
                    rng,
                    scene.origin + V3::new(side * 120.0, 40.0, 20.0),
                    26,
                    700.0,
                    0.7,
                    hot,
                    0.5,
                );
                if beat.is_multiple_of(3) {
                    ring(
                        particles,
                        scene.origin,
                        RingShape {
                            color: hot,
                            radius: 90.0,
                            speed: 1100.0,
                            thickness: 12.0,
                            life: 0.8,
                            tilt: 0.25,
                            spin: elapsed * 0.3,
                        },
                    );
                }
            }
            Self::Jackpot => {
                particles.cannon_confetti(
                    rng,
                    scene.origin + V3::new(side * 900.0, -280.0, -60.0),
                    V3::new(-side * 0.7, 1.0, 0.08),
                    34,
                    1450.0,
                    0.22,
                    &scene.palette,
                );
                particles.fountain_embers(
                    rng,
                    scene.origin - V3::new(0.0, 200.0, 0.0),
                    V3::new(0.0, 1.0, 0.0),
                    14,
                    440.0,
                    0.32,
                    hot,
                );
                particles.burst_sparks(rng, scene.origin, 34, 900.0, 0.8, hot, 0.6);
                ring(
                    particles,
                    scene.origin,
                    RingShape {
                        color: if beat.is_multiple_of(2) {
                            scene.tier_color
                        } else {
                            hot
                        },
                        radius: 110.0,
                        speed: 1250.0,
                        thickness: 14.0,
                        life: 1.0,
                        tilt: side * 0.4,
                        spin: elapsed * 0.5,
                    },
                );
            }
        }
    }
}

struct RingShape {
    color: AppColor,
    radius: f32,
    speed: f32,
    thickness: f32,
    life: f32,
    tilt: f32,
    spin: f32,
}

fn ring(particles: &mut ParticleSystem, center: V3, shape: RingShape) {
    particles.push_shockwave(Shockwave {
        center,
        radius: shape.radius,
        speed: shape.speed,
        thickness: shape.thickness,
        color: shape.color,
        life: shape.life,
        max_life: shape.life,
        tilt: shape.tilt,
        spin: shape.spin,
    });
}

fn lighten(color: AppColor, amount: f32) -> AppColor {
    let channel = |value: u8| {
        let level = value as f32 / 255.0;
        ((level + (1.0 - level) * amount) * 255.0).round() as u8
    };
    AppColor::from_argb(
        color.a,
        channel(color.r),
        channel(color.g),
        channel(color.b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> CelebrationScene {
        CelebrationScene {
            origin: V3::new(0.0, 0.0, 60.0),
            tier_color: AppColor::from_rgb(235, 75, 75),
            palette: vec![
                AppColor::from_rgb(255, 255, 255),
                AppColor::from_rgb(235, 75, 75),
            ],
            intensity: 0.75,
        }
    }

    const LADDER: [Celebration; 5] = [
        Celebration::Clean,
        Celebration::Plume,
        Celebration::Streamers,
        Celebration::Shatter,
        Celebration::Jackpot,
    ];

    #[test]
    fn a_five_tier_ladder_uses_every_recipe_in_order() {
        let picked: Vec<Celebration> = (0..5)
            .map(|rank| Celebration::for_intensity(rank as f32 / 4.0))
            .collect();
        assert_eq!(picked, LADDER.to_vec());
    }

    #[test]
    fn the_ends_of_the_ladder_are_pinned() {
        assert_eq!(Celebration::for_intensity(0.0), Celebration::Clean);
        assert_eq!(Celebration::for_intensity(1.0), Celebration::Jackpot);
        // A single-tier config reports intensity 1 and should still be loud.
        assert_eq!(Celebration::for_intensity(1.5), Celebration::Jackpot);
    }

    #[test]
    fn every_recipe_emits_something_and_asks_for_a_flash() {
        for celebration in LADDER {
            let mut particles = ParticleSystem::default();
            let mut rng = Rng::from_seed(7);
            let flash = celebration.fire(&mut particles, &mut rng, &scene(), 1.0);
            assert!(
                particles.live_count() > 0,
                "{} emitted no particles",
                celebration.label()
            );
            assert!(
                flash.strength > 0.0 && flash.strength <= 1.0,
                "{} asked for a flash of {}",
                celebration.label(),
                flash.strength
            );
        }
    }

    #[test]
    fn the_recipes_escalate_up_the_ladder() {
        let mut previous = 0usize;
        let mut previous_flash = 0.0f32;
        for celebration in LADDER {
            let mut particles = ParticleSystem::default();
            let mut rng = Rng::from_seed(11);
            let flash = celebration.fire(&mut particles, &mut rng, &scene(), 1.0);
            let live = particles.live_count();
            assert!(
                live > previous,
                "{} emitted {live}, no more than the tier below it",
                celebration.label()
            );
            assert!(
                flash.strength > previous_flash,
                "{} flashed no brighter than the tier below it",
                celebration.label()
            );
            previous = live;
            previous_flash = flash.strength;
        }
    }

    #[test]
    fn each_recipe_has_its_own_mix_of_particle_kinds() {
        use std::collections::BTreeSet;
        let mut signatures = BTreeSet::new();
        for celebration in LADDER {
            let mut particles = ParticleSystem::default();
            let mut rng = Rng::from_seed(3);
            celebration.fire(&mut particles, &mut rng, &scene(), 1.0);
            let kinds: BTreeSet<&'static str> = particles
                .particles
                .iter()
                .map(|particle| match particle.kind {
                    crate::particles::ParticleKind::Spark => "spark",
                    crate::particles::ParticleKind::Ember => "ember",
                    crate::particles::ParticleKind::Dust => "dust",
                    crate::particles::ParticleKind::Confetti => "confetti",
                    crate::particles::ParticleKind::Shard => "shard",
                    crate::particles::ParticleKind::Streak => "streak",
                })
                .collect();
            let signature = (
                kinds.into_iter().collect::<Vec<_>>().join("+"),
                particles.shockwaves.len(),
            );
            assert!(
                signatures.insert(signature.clone()),
                "{} looks like another tier: {signature:?}",
                celebration.label()
            );
        }
    }

    #[test]
    fn a_beat_keeps_the_loud_tiers_going() {
        for celebration in LADDER {
            let mut particles = ParticleSystem::default();
            let mut rng = Rng::from_seed(5);
            celebration.beat(&mut particles, &mut rng, &scene(), 0, 1.0);
            assert!(
                particles.live_count() > 0,
                "{} went quiet during the hold",
                celebration.label()
            );
        }
    }

    #[test]
    fn force_scales_a_recipe_up_for_the_wheel() {
        let mut normal = ParticleSystem::default();
        let mut loud = ParticleSystem::default();
        let flash_normal =
            Celebration::Jackpot.fire(&mut normal, &mut Rng::from_seed(9), &scene(), 1.0);
        let flash_loud =
            Celebration::Jackpot.fire(&mut loud, &mut Rng::from_seed(9), &scene(), 1.8);
        assert!(loud.live_count() > normal.live_count());
        assert!(flash_loud.strength >= flash_normal.strength);
    }
}
