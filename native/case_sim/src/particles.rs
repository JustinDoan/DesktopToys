//! A small pooled particle system for the case sequence.
//!
//! Everything lives in case space and is billboarded or tumbled by the
//! renderer. Emission is capped so a long queue of openings can never grow the
//! vertex buffer without bound.

use core_types::AppColor;

use crate::math::V3;
use crate::rng::Rng;

pub const MAX_PARTICLES: usize = 900;
pub const MAX_SHOCKWAVES: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    /// Bright additive point that decays fast. The workhorse.
    Spark,
    /// Slow, flickering, drifts upward. Used for the reveal's ambience.
    Ember,
    /// Barely visible motes that give the empty space some depth.
    Dust,
    /// Flat tumbling rectangle lit like solid geometry.
    Confetti,
    /// Metal fragment from the case cracking open.
    Shard,
    /// Velocity-stretched glow, used for reel speed lines.
    Streak,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub kind: ParticleKind,
    pub position: V3,
    pub velocity: V3,
    pub color: AppColor,
    pub size: f32,
    /// Extra emissive punch above the base colour.
    pub glow: f32,
    pub life: f32,
    pub max_life: f32,
    /// Per-particle randomness so the shader can vary flicker and shape.
    pub seed: f32,
    /// Current tumble angle and its rate, for the flat kinds.
    pub spin: f32,
    pub spin_rate: f32,
    pub tumble_axis: V3,
    gravity: f32,
    drag: f32,
    /// Pull toward the system's attractor, in units per second squared.
    attraction: f32,
}

impl Particle {
    /// 1 when freshly spawned, falling to 0 at death.
    pub fn remaining(&self) -> f32 {
        if self.max_life <= 0.0 {
            0.0
        } else {
            (self.life / self.max_life).clamp(0.0, 1.0)
        }
    }

    /// 0 when freshly spawned, rising to 1 at death.
    pub fn age(&self) -> f32 {
        1.0 - self.remaining()
    }

    /// Opacity envelope, shaped per kind.
    pub fn fade(&self) -> f32 {
        let remaining = self.remaining();
        match self.kind {
            // Sparks and streaks want a hard bright head and a quick tail.
            ParticleKind::Spark | ParticleKind::Streak => remaining * remaining,
            // Dust eases in as well as out so motes don't pop into existence.
            ParticleKind::Dust => {
                let fade_in = (self.age() * 6.0).clamp(0.0, 1.0);
                fade_in * remaining
            }
            ParticleKind::Ember => remaining.sqrt() * remaining,
            ParticleKind::Confetti | ParticleKind::Shard => (remaining * 3.0).clamp(0.0, 1.0),
        }
    }
}

/// An expanding ring. Kept separate from particles because it is one piece of
/// geometry rather than a point, and it needs its own orientation.
#[derive(Clone, Copy, Debug)]
pub struct Shockwave {
    pub center: V3,
    pub radius: f32,
    pub speed: f32,
    pub thickness: f32,
    pub color: AppColor,
    pub life: f32,
    pub max_life: f32,
    /// Tilt away from screen-facing, in radians, so rings read as 3D discs.
    pub tilt: f32,
    pub spin: f32,
}

impl Shockwave {
    pub fn remaining(&self) -> f32 {
        if self.max_life <= 0.0 {
            0.0
        } else {
            (self.life / self.max_life).clamp(0.0, 1.0)
        }
    }
}

#[derive(Default)]
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pub shockwaves: Vec<Shockwave>,
    /// Point that `attraction` pulls toward.
    pub attractor: V3,
}

impl ParticleSystem {
    pub fn new() -> Self {
        Self {
            particles: Vec::with_capacity(MAX_PARTICLES),
            shockwaves: Vec::with_capacity(MAX_SHOCKWAVES),
            attractor: V3::ZERO,
        }
    }

    pub fn clear(&mut self) {
        self.particles.clear();
        self.shockwaves.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.particles.is_empty() && self.shockwaves.is_empty()
    }

    pub fn live_count(&self) -> usize {
        self.particles.len()
    }

    pub fn update(&mut self, dt: f32) {
        let attractor = self.attractor;
        self.particles.retain_mut(|particle| {
            particle.life -= dt;
            if particle.life <= 0.0 {
                return false;
            }

            if particle.attraction != 0.0 {
                let toward = attractor - particle.position;
                let distance = toward.length().max(24.0);
                // Inverse-square pull, so distant sparks drift and close ones snap.
                particle.velocity +=
                    toward * (particle.attraction / (distance * distance) * dt * 1000.0);
            }
            particle.velocity.y -= particle.gravity * dt;
            let damping = (1.0 - particle.drag * dt).clamp(0.0, 1.0);
            particle.velocity = particle.velocity * damping;
            particle.position += particle.velocity * dt;
            particle.spin += particle.spin_rate * dt;
            true
        });

        self.shockwaves.retain_mut(|wave| {
            wave.life -= dt;
            if wave.life <= 0.0 {
                return false;
            }
            // Rings decelerate as they widen, which reads as air resistance.
            wave.radius += wave.speed * dt;
            wave.speed *= (1.0 - 2.4 * dt).clamp(0.0, 1.0);
            wave.spin += dt * 0.6;
            true
        });
    }

    fn push(&mut self, particle: Particle) {
        if self.particles.len() >= MAX_PARTICLES {
            // Drop the oldest rather than the newest: the freshest burst is
            // always the one the viewer is looking at.
            let oldest = self
                .particles
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.remaining().total_cmp(&b.remaining()))
                .map(|(index, _)| index);
            match oldest {
                Some(index) => self.particles[index] = particle,
                None => return,
            }
        } else {
            self.particles.push(particle);
        }
    }

    pub fn push_shockwave(&mut self, wave: Shockwave) {
        if self.shockwaves.len() >= MAX_SHOCKWAVES {
            self.shockwaves.remove(0);
        }
        self.shockwaves.push(wave);
    }

    /// Radial burst of additive sparks.
    #[allow(clippy::too_many_arguments)]
    pub fn burst_sparks(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        count: usize,
        speed: f32,
        spread: f32,
        color: AppColor,
        life: f32,
    ) {
        for _ in 0..count {
            let direction = random_direction(rng);
            // Bias outward on the screen plane so bursts read wide, not deep.
            let direction = V3::new(direction.x, direction.y, direction.z * 0.45).normalized();
            let velocity = direction * (speed * rng.range(1.0 - spread, 1.0 + spread));
            let max_life = life * rng.range(0.55, 1.25);
            self.push(Particle {
                kind: ParticleKind::Spark,
                position: origin + direction * rng.range(0.0, 18.0),
                velocity,
                color,
                size: rng.range(2.4, 7.5),
                glow: rng.range(1.2, 2.6),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: rng.range(60.0, 240.0),
                drag: rng.range(1.6, 3.4),
                attraction: 0.0,
            });
        }
    }

    /// Long-lived floating embers for the result hold.
    pub fn spawn_embers(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        count: usize,
        radius: f32,
        color: AppColor,
    ) {
        for _ in 0..count {
            let direction = random_direction(rng);
            let max_life = rng.range(1.8, 4.2);
            self.push(Particle {
                kind: ParticleKind::Ember,
                position: origin + direction * rng.range(radius * 0.3, radius),
                velocity: V3::new(
                    rng.range(-26.0, 26.0),
                    rng.range(8.0, 54.0),
                    rng.range(-14.0, 14.0),
                ),
                color,
                size: rng.range(2.0, 5.0),
                glow: rng.range(0.8, 1.9),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: rng.range(-30.0, -6.0),
                drag: rng.range(0.5, 1.4),
                attraction: 0.0,
            });
        }
    }

    /// Ambient motes that drift through the whole sequence.
    pub fn spawn_dust(&mut self, rng: &mut Rng, count: usize, extent: V3, color: AppColor) {
        for _ in 0..count {
            let max_life = rng.range(2.5, 7.0);
            self.push(Particle {
                kind: ParticleKind::Dust,
                position: V3::new(
                    rng.signed() * extent.x,
                    rng.signed() * extent.y,
                    rng.signed() * extent.z,
                ),
                velocity: V3::new(
                    rng.range(-16.0, 16.0),
                    rng.range(-10.0, 22.0),
                    rng.range(-8.0, 8.0),
                ),
                color,
                size: rng.range(1.4, 3.4),
                glow: rng.range(0.25, 0.8),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: 0.0,
                drag: 0.25,
                attraction: 0.0,
            });
        }
    }

    /// Sparks that spiral inward, used while the case charges up.
    pub fn spawn_infalling(
        &mut self,
        rng: &mut Rng,
        target: V3,
        count: usize,
        radius: f32,
        color: AppColor,
    ) {
        self.attractor = target;
        for _ in 0..count {
            let direction = random_direction(rng);
            let position = target + direction * rng.range(radius * 0.7, radius);
            // Give it tangential velocity so it orbits in before it lands.
            let tangent = direction.cross(V3::new(0.0, 1.0, 0.0)).normalized();
            let max_life = rng.range(0.7, 1.5);
            self.push(Particle {
                kind: ParticleKind::Spark,
                position,
                velocity: tangent * rng.range(90.0, 260.0),
                color,
                size: rng.range(2.0, 5.2),
                glow: rng.range(1.0, 2.2),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: 0.0,
                drag: 0.4,
                attraction: rng.range(26.0, 62.0),
            });
        }
    }

    /// Tumbling metal fragments from the case shell.
    pub fn burst_shards(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        count: usize,
        speed: f32,
        color: AppColor,
    ) {
        for _ in 0..count {
            let direction = random_direction(rng);
            let max_life = rng.range(1.1, 2.3);
            self.push(Particle {
                kind: ParticleKind::Shard,
                position: origin + direction * rng.range(10.0, 40.0),
                velocity: direction * (speed * rng.range(0.5, 1.4)) + V3::new(0.0, 120.0, 0.0),
                color,
                size: rng.range(5.0, 16.0),
                glow: rng.range(0.0, 0.35),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: rng.range(0.0, std::f32::consts::TAU),
                spin_rate: rng.range(-9.0, 9.0),
                tumble_axis: random_direction(rng),
                gravity: rng.range(900.0, 1500.0),
                drag: 0.8,
                attraction: 0.0,
            });
        }
    }

    /// Paper confetti for the loud tiers.
    pub fn burst_confetti(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        count: usize,
        speed: f32,
        palette: &[AppColor],
    ) {
        if palette.is_empty() {
            return;
        }
        for _ in 0..count {
            let direction = random_direction(rng);
            let max_life = rng.range(2.0, 4.0);
            self.push(Particle {
                kind: ParticleKind::Confetti,
                position: origin + direction * rng.range(0.0, 60.0),
                velocity: V3::new(
                    direction.x * speed * rng.range(0.4, 1.2),
                    speed * rng.range(0.6, 1.5),
                    direction.z * speed * rng.range(0.2, 0.7),
                ),
                color: palette[rng.below(palette.len())],
                size: rng.range(6.0, 14.0),
                glow: 0.0,
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: rng.range(0.0, std::f32::consts::TAU),
                spin_rate: rng.range(-11.0, 11.0),
                tumble_axis: random_direction(rng),
                gravity: rng.range(520.0, 900.0),
                drag: rng.range(1.2, 2.6),
                attraction: 0.0,
            });
        }
    }

    /// Confetti thrown one way instead of filling a sphere, for cannons firing
    /// in from the edges of the stage. `spread` of 0 is a beam, 1 a wide fan.
    #[allow(clippy::too_many_arguments)]
    pub fn cannon_confetti(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        aim: V3,
        count: usize,
        speed: f32,
        spread: f32,
        palette: &[AppColor],
    ) {
        if palette.is_empty() {
            return;
        }
        let aim = aim.normalized();
        for _ in 0..count {
            let direction = (aim + random_direction(rng) * spread).normalized();
            let max_life = rng.range(2.2, 4.4);
            self.push(Particle {
                kind: ParticleKind::Confetti,
                position: origin + random_direction(rng) * rng.range(0.0, 40.0),
                velocity: direction * (speed * rng.range(0.7, 1.3)),
                color: palette[rng.below(palette.len())],
                size: rng.range(6.0, 15.0),
                glow: 0.0,
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: rng.range(0.0, std::f32::consts::TAU),
                spin_rate: rng.range(-13.0, 13.0),
                tumble_axis: random_direction(rng),
                gravity: rng.range(480.0, 820.0),
                drag: rng.range(1.0, 2.2),
                attraction: 0.0,
            });
        }
    }

    /// Embers pushed along an axis, for plumes and columns that climb rather
    /// than hang. They keep the ember's negative gravity so they keep rising.
    #[allow(clippy::too_many_arguments)]
    pub fn fountain_embers(
        &mut self,
        rng: &mut Rng,
        origin: V3,
        aim: V3,
        count: usize,
        speed: f32,
        spread: f32,
        color: AppColor,
    ) {
        let aim = aim.normalized();
        for _ in 0..count {
            let direction = (aim + random_direction(rng) * spread).normalized();
            let max_life = rng.range(1.6, 3.6);
            self.push(Particle {
                kind: ParticleKind::Ember,
                position: origin + random_direction(rng) * rng.range(0.0, 30.0),
                velocity: direction * (speed * rng.range(0.6, 1.4)),
                color,
                size: rng.range(2.2, 5.6),
                glow: rng.range(1.0, 2.2),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: rng.range(-40.0, -8.0),
                drag: rng.range(0.4, 1.1),
                attraction: 0.0,
            });
        }
    }

    /// Horizontal speed lines that sell the reel's velocity.
    pub fn spawn_streaks(
        &mut self,
        rng: &mut Rng,
        count: usize,
        extent: V3,
        speed: f32,
        color: AppColor,
    ) {
        for _ in 0..count {
            let max_life = rng.range(0.16, 0.42);
            self.push(Particle {
                kind: ParticleKind::Streak,
                position: V3::new(
                    rng.range(extent.x * 0.4, extent.x),
                    rng.signed() * extent.y,
                    rng.signed() * extent.z,
                ),
                velocity: V3::new(-speed * rng.range(0.7, 1.3), rng.range(-20.0, 20.0), 0.0),
                color,
                size: rng.range(1.6, 4.0),
                glow: rng.range(0.7, 1.8),
                life: max_life,
                max_life,
                seed: rng.unit(),
                spin: 0.0,
                spin_rate: 0.0,
                tumble_axis: V3::new(0.0, 1.0, 0.0),
                gravity: 0.0,
                drag: 0.0,
                attraction: 0.0,
            });
        }
    }
}

fn random_direction(rng: &mut Rng) -> V3 {
    // Uniform on the sphere, so bursts don't clump at the poles.
    let z = rng.signed();
    let angle = rng.unit() * std::f32::consts::TAU;
    let planar = (1.0 - z * z).max(0.0).sqrt();
    V3::new(planar * angle.cos(), planar * angle.sin(), z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_color() -> AppColor {
        AppColor::from_rgb(255, 200, 80)
    }

    #[test]
    fn particles_expire() {
        let mut system = ParticleSystem::new();
        let mut rng = Rng::from_seed(3);
        system.burst_sparks(&mut rng, V3::ZERO, 50, 400.0, 0.4, test_color(), 0.5);
        assert_eq!(system.particles.len(), 50);
        for _ in 0..200 {
            system.update(1.0 / 60.0);
        }
        assert!(system.particles.is_empty(), "sparks should have died out");
    }

    #[test]
    fn emission_respects_the_pool_cap() {
        let mut system = ParticleSystem::new();
        let mut rng = Rng::from_seed(5);
        for _ in 0..40 {
            system.burst_sparks(&mut rng, V3::ZERO, 200, 300.0, 0.3, test_color(), 4.0);
        }
        assert_eq!(system.particles.len(), MAX_PARTICLES);
    }

    #[test]
    fn shockwaves_expand_then_expire() {
        let mut system = ParticleSystem::new();
        system.push_shockwave(Shockwave {
            center: V3::ZERO,
            radius: 10.0,
            speed: 800.0,
            thickness: 12.0,
            color: test_color(),
            life: 0.6,
            max_life: 0.6,
            tilt: 0.0,
            spin: 0.0,
        });
        system.update(0.1);
        assert!(system.shockwaves[0].radius > 10.0);
        for _ in 0..60 {
            system.update(1.0 / 60.0);
        }
        assert!(system.shockwaves.is_empty());
    }

    #[test]
    fn shockwave_list_stays_bounded() {
        let mut system = ParticleSystem::new();
        for _ in 0..MAX_SHOCKWAVES * 3 {
            system.push_shockwave(Shockwave {
                center: V3::ZERO,
                radius: 1.0,
                speed: 100.0,
                thickness: 4.0,
                color: test_color(),
                life: 10.0,
                max_life: 10.0,
                tilt: 0.0,
                spin: 0.0,
            });
        }
        assert_eq!(system.shockwaves.len(), MAX_SHOCKWAVES);
    }

    #[test]
    fn infalling_sparks_close_on_the_attractor() {
        let mut system = ParticleSystem::new();
        let mut rng = Rng::from_seed(11);
        let target = V3::new(0.0, 0.0, 0.0);
        system.spawn_infalling(&mut rng, target, 30, 320.0, test_color());
        let before = average_distance(&system, target);
        for _ in 0..30 {
            system.update(1.0 / 60.0);
        }
        let after = average_distance(&system, target);
        assert!(after < before, "expected {after} to close in from {before}");
    }

    #[test]
    fn fade_envelopes_start_and_end_dark() {
        let mut system = ParticleSystem::new();
        let mut rng = Rng::from_seed(13);
        system.spawn_dust(&mut rng, 8, V3::splat(200.0), test_color());
        for particle in &system.particles {
            assert!(particle.fade() < 0.05, "dust should fade in");
        }
        for particle in &mut system.particles {
            particle.life = particle.max_life * 0.001;
        }
        for particle in &system.particles {
            assert!(particle.fade() < 0.05, "dust should fade out");
        }
    }

    fn average_distance(system: &ParticleSystem, target: V3) -> f32 {
        let total: f32 = system
            .particles
            .iter()
            .map(|particle| (particle.position - target).length())
            .sum();
        total / system.particles.len() as f32
    }
}
