//! Geometry for the CS:GO-style case opening.
//!
//! The rest of the overlay leans on the vertex shader's perspective divide,
//! which is fixed to the screen centre and has no view matrix. That is fine for
//! desktop toys but useless for a cinematic sequence, so the case stage runs its
//! own CPU projection instead: everything is projected through
//! [`case_sim::CameraRig`], emitted at a screen-space Z so the vertex shader
//! leaves it alone, and sorted back to front. Painter's ordering means the
//! hundreds of additive particles and glass-like cards blend correctly without
//! needing a second draw call or a depth-write toggle.
//!
//! Shading is split: the CPU does layout, lighting of solid surfaces, and
//! billboarding; the pixel shader does the expensive per-pixel work (brushed
//! metal, holographic card sheen, soft particle falloff, anti-aliased text).
//! Those shaders live next to the rest in `native/app/src/main.rs`.
//!
//! Every material here packs `material = [id, u, v, param]`, with `param` and
//! `material_extra` meaning something different per id:
//!
//! | id | name  | `material.w`  | `material_extra`                      |
//! |----|-------|---------------|---------------------------------------|
//! | 10 | glow  | shape         | `[intensity, falloff, taper/seed, t]` |
//! | 11 | card  | tier position | `[t, highlight, seed, dim]`           |
//! | 12 | shell | seam glow     | `[view normal xyz, surface kind]`     |
//! | 13 | ray   | edge softness | `[intensity, unused, ray index, t]`   |
//! | 14 | glyph | unused        | `[rows 6-7, cell x, cell y, weight]`  |
//!
//! Id 14 is the exception to the UV rule: it packs the first six rows of the
//! glyph bitmask into `material.yzw`, reusing the layout the chat glyph
//! material already established, and carries cell coordinates in `_extra.yz`.

use case_sim::{
    CARD_DEPTH, CARD_HEIGHT, CARD_PITCH, CARD_WIDTH, CameraRig, CasePhase, CaseSession,
    ParticleKind, REEL_HALF_SPAN, REEL_RADIUS, ReelSlot, V3, WHEEL_CENTER, WHEEL_PANEL_HALF,
    wheel_radius,
};
use core_types::{AppColor, RectF};
use font8x8::UnicodeFonts;

use crate::{GpuVertex, color_to_f32};

pub(crate) const CASE_GLOW_MATERIAL_ID: f32 = 10.0;
pub(crate) const CASE_CARD_MATERIAL_ID: f32 = 11.0;
pub(crate) const CASE_SHELL_MATERIAL_ID: f32 = 12.0;
pub(crate) const CASE_RAY_MATERIAL_ID: f32 = 13.0;
pub(crate) const CASE_GLYPH_MATERIAL_ID: f32 = 14.0;

/// Glow shapes, passed in `material.w`. Sprite and box read UVs in -1..1; band
/// puts its thin axis on U in -1..1 and its run on V in 0..1.
const SHAPE_SPRITE: f32 = 0.0;
const SHAPE_VIGNETTE: f32 = 1.0;
const SHAPE_BOX: f32 = 2.0;
const SHAPE_BAND: f32 = 3.0;

/// Shell surface kinds, passed in `material_extra.w`.
const SHELL_PANEL: f32 = 0.0;
const SHELL_SEAM: f32 = 1.0;
const SHELL_ETCH: f32 = 2.0;

/// Anything at or past 800 skips the overlay's own perspective divide and is
/// treated as already-projected pixels.
const CASE_Z: f32 = 900.0;
/// Case-space units in front of the eye before a vertex is dropped.
const NEAR_PLANE: f32 = 60.0;
/// How far forward text sorts relative to the surface it is printed on.
const TEXT_SORT_BIAS: f32 = 6.0;
/// Glyph advance in cells of the 8x8 grid. font8x8's widest characters fill
/// seven of the eight columns and the shader's box filter spreads ink about half
/// a cell past the body, so a tighter advance has neighbours touching.
const GLYPH_ADVANCE_CELLS: f32 = 7.6;

const CASE_HALF: V3 = V3::new(152.0, 104.0, 104.0);
const CASE_BEVEL: f32 = 15.0;
const LID_THICKNESS: f32 = 26.0;

/// Emits the whole sequence. Call once per frame, after world objects and
/// before the HUD panels so debug text stays legible on top.
///
/// `area` is the surface-local rectangle to stage the sequence inside, which is
/// one monitor rather than the whole surface: an overlay spanning two screens
/// would otherwise centre the case on the bezel between them.
pub(crate) fn emit_case_opening(out: &mut Vec<GpuVertex>, area: RectF, session: &CaseSession) {
    let stage = &session.stage;
    if stage.global_fade <= 0.002 || stage.phase == CasePhase::Done {
        return;
    }

    emit_backdrop(out, area, stage.backdrop * stage.global_fade);

    let camera = Camera::new(&stage.camera, area);
    let mut painter = Painter::default();

    emit_case_shell(&mut painter, &camera, session);
    emit_light_pillar(&mut painter, &camera, session);
    emit_reel(&mut painter, &camera, session);
    emit_ticker(&mut painter, &camera, session);
    emit_winner(&mut painter, &camera, session);
    emit_wheel(&mut painter, &camera, session);
    emit_shockwaves(&mut painter, &camera, session);
    emit_particles(&mut painter, &camera, session);
    emit_headings(&mut painter, &camera, session);

    painter.flush(out);

    // The flash sits above everything, unsorted.
    emit_flash(
        out,
        area,
        stage.flash * stage.global_fade,
        stage.flash_color,
    );
}

// ---------------------------------------------------------------------------
// Camera and painter
// ---------------------------------------------------------------------------

struct Camera {
    eye: V3,
    right: V3,
    up: V3,
    forward: V3,
    focal: f32,
    center_x: f32,
    center_y: f32,
    roll_sin: f32,
    roll_cos: f32,
}

/// A point after projection. `depth` is distance along the view axis, used only
/// as the painter's sort key. Sizes need no correction here because billboards
/// are built in case space from the camera basis, so perspective scales them.
#[derive(Clone, Copy)]
struct Projected {
    x: f32,
    y: f32,
    depth: f32,
}

impl Camera {
    fn new(rig: &CameraRig, area: RectF) -> Self {
        let forward = (rig.target - rig.eye).normalized();
        let mut right = forward.cross(rig.up);
        if right.length() < 1e-4 {
            // Looking straight up or down; any perpendicular will do.
            right = V3::new(1.0, 0.0, 0.0);
        }
        let right = right.normalized();
        let up = right.cross(forward).normalized();
        let half_fov = (rig.fov_y * 0.5).clamp(0.05, 1.35);
        let (roll_sin, roll_cos) = rig.roll.sin_cos();
        Self {
            eye: rig.eye,
            right,
            up,
            forward,
            focal: (area.height.max(1.0) * 0.5) / half_fov.tan(),
            center_x: area.x + area.width * 0.5,
            center_y: area.y + area.height * 0.5,
            roll_sin,
            roll_cos,
        }
    }

    fn project(&self, point: V3) -> Option<Projected> {
        let view = point - self.eye;
        let depth = view.dot(self.forward);
        if depth < NEAR_PLANE {
            return None;
        }
        let scale = self.focal / depth;
        let offset_x = view.dot(self.right) * scale;
        // Screen Y grows downward while case space has Y up.
        let offset_y = -view.dot(self.up) * scale;
        let (x, y) = (
            offset_x * self.roll_cos - offset_y * self.roll_sin,
            offset_x * self.roll_sin + offset_y * self.roll_cos,
        );
        Some(Projected {
            x: self.center_x + x,
            y: self.center_y + y,
            depth,
        })
    }

    /// Screen-space axes for a quad that always faces the eye.
    fn billboard_axes(&self) -> (V3, V3) {
        (self.right, self.up)
    }
}

/// Collects triangles with a sort key, then replays them far-to-near.
#[derive(Default)]
struct Painter {
    vertices: Vec<GpuVertex>,
    triangles: Vec<(f32, u32)>,
}

impl Painter {
    fn triangle(&mut self, a: GpuVertex, b: GpuVertex, c: GpuVertex, depth: f32) {
        let start = self.vertices.len() as u32;
        self.vertices.push(a);
        self.vertices.push(b);
        self.vertices.push(c);
        self.triangles.push((depth, start));
    }

    fn flush(&mut self, out: &mut Vec<GpuVertex>) {
        // Farthest first, so nearer translucent surfaces blend over them.
        self.triangles
            .sort_unstable_by(|left, right| right.0.total_cmp(&left.0));
        out.reserve(self.triangles.len() * 3);
        for &(_, start) in &self.triangles {
            let start = start as usize;
            out.extend_from_slice(&self.vertices[start..start + 3]);
        }
    }
}

/// A rotation, uniform scale and translation, applied in that order.
#[derive(Clone, Copy)]
struct Placement {
    rotation: V3,
    scale: f32,
    position: V3,
}

impl Placement {
    fn new(position: V3) -> Self {
        Self {
            rotation: V3::ZERO,
            scale: 1.0,
            position,
        }
    }

    fn direction(&self, local: V3) -> V3 {
        local
            .rotate_x(self.rotation.x)
            .rotate_y(self.rotation.y)
            .rotate_z(self.rotation.z)
    }

    fn point(&self, local: V3) -> V3 {
        self.direction(local * self.scale) + self.position
    }
}

fn vertex(point: &Projected, color: [f32; 4], material: [f32; 4], extra: [f32; 4]) -> GpuVertex {
    GpuVertex {
        position: [point.x, point.y, CASE_Z],
        color,
        material,
        material_extra: extra,
    }
}

/// Projects a quad in case space and emits it as two triangles.
///
/// `uvs` are handed to the pixel shader in `material.yz`. Returns false when
/// any corner is behind the near plane, which is the caller's cue that the
/// piece was skipped.
#[allow(clippy::too_many_arguments)]
fn quad(
    painter: &mut Painter,
    camera: &Camera,
    corners: [V3; 4],
    uvs: [[f32; 2]; 4],
    color: [f32; 4],
    material_id: f32,
    param: f32,
    extra: [f32; 4],
) -> bool {
    if color[3] <= 0.002 {
        return true;
    }
    let mut projected = [None; 4];
    for (slot, corner) in projected.iter_mut().zip(corners.iter()) {
        *slot = camera.project(*corner);
    }
    let Some(points) = collect4(projected) else {
        return false;
    };
    let depth = (points[0].depth + points[1].depth + points[2].depth + points[3].depth) * 0.25;
    let build = |index: usize| {
        vertex(
            &points[index],
            color,
            [material_id, uvs[index][0], uvs[index][1], param],
            extra,
        )
    };
    painter.triangle(build(0), build(1), build(2), depth);
    painter.triangle(build(0), build(2), build(3), depth);
    true
}

/// A flat-shaded triangle, used for chamfer corners and ray fans.
#[allow(clippy::too_many_arguments)]
fn triangle(
    painter: &mut Painter,
    camera: &Camera,
    corners: [V3; 3],
    uvs: [[f32; 2]; 3],
    color: [f32; 4],
    material_id: f32,
    param: f32,
    extra: [f32; 4],
) {
    if color[3] <= 0.002 {
        return;
    }
    let points = [
        camera.project(corners[0]),
        camera.project(corners[1]),
        camera.project(corners[2]),
    ];
    let (Some(a), Some(b), Some(c)) = (points[0], points[1], points[2]) else {
        return;
    };
    let depth = (a.depth + b.depth + c.depth) / 3.0;
    let build = |point: &Projected, uv: [f32; 2]| {
        vertex(point, color, [material_id, uv[0], uv[1], param], extra)
    };
    painter.triangle(
        build(&a, uvs[0]),
        build(&b, uvs[1]),
        build(&c, uvs[2]),
        depth,
    );
}

fn collect4(slots: [Option<Projected>; 4]) -> Option<[Projected; 4]> {
    Some([slots[0]?, slots[1]?, slots[2]?, slots[3]?])
}

const UNIT_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
const SPRITE_UVS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];

// ---------------------------------------------------------------------------
// Screen-space layers
// ---------------------------------------------------------------------------

/// Corners of the stage, wound clockwise from the top left.
fn area_corners(area: RectF) -> [[f32; 2]; 4] {
    [
        [area.x, area.y],
        [area.right(), area.y],
        [area.right(), area.bottom()],
        [area.x, area.bottom()],
    ]
}

/// A radial vignette that pulls attention to the middle of the stage without
/// fully hiding the desktop underneath.
fn emit_backdrop(out: &mut Vec<GpuVertex>, area: RectF, strength: f32) {
    if strength <= 0.002 {
        return;
    }
    let corners = area_corners(area);
    let color = [0.02, 0.03, 0.05, strength];
    for index in [0usize, 1, 2, 0, 2, 3] {
        out.push(GpuVertex {
            position: [corners[index][0], corners[index][1], CASE_Z],
            color,
            material: [
                CASE_GLOW_MATERIAL_ID,
                SPRITE_UVS[index][0],
                SPRITE_UVS[index][1],
                SHAPE_VIGNETTE,
            ],
            material_extra: [1.0, 1.0, 0.0, 0.0],
        });
    }
}

/// Stage-wide additive flash for impacts.
fn emit_flash(out: &mut Vec<GpuVertex>, area: RectF, strength: f32, color: AppColor) {
    if strength <= 0.002 {
        return;
    }
    let corners = area_corners(area);
    let tint = color_to_f32(color, 255);
    let color = [tint[0], tint[1], tint[2], strength.min(1.0)];
    for index in [0usize, 1, 2, 0, 2, 3] {
        out.push(GpuVertex {
            position: [corners[index][0], corners[index][1], CASE_Z],
            color,
            material: [
                CASE_GLOW_MATERIAL_ID,
                SPRITE_UVS[index][0] * 0.55,
                SPRITE_UVS[index][1] * 0.55,
                SHAPE_SPRITE,
            ],
            material_extra: [1.0, 0.7, 0.0, 0.0],
        });
    }
}

// ---------------------------------------------------------------------------
// The case itself
// ---------------------------------------------------------------------------

fn emit_case_shell(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    if !stage.case_visible || stage.case_opacity <= 0.002 {
        return;
    }
    let alpha = stage.case_opacity * stage.global_fade;
    let seam = stage.case_seam_glow;
    let place = Placement {
        rotation: stage.case_rotation,
        scale: stage.case_scale,
        position: stage.case_position,
    };

    // Body: everything below the lid split.
    let body_half = V3::new(CASE_HALF.x, CASE_HALF.y - LID_THICKNESS, CASE_HALF.z);
    let body_offset = V3::new(0.0, -LID_THICKNESS, 0.0);
    push_chamfer_box(
        painter,
        camera,
        &|local| place.point(local + body_offset),
        &|local| place.direction(local),
        body_half,
        CASE_BEVEL,
        CASE_BODY_COLOR,
        seam,
        alpha,
    );

    // Etched panels on the four sides, so the crate reads as built rather than
    // extruded.
    for (axis, sign) in [(0usize, 1.0f32), (0, -1.0), (2, 1.0), (2, -1.0)] {
        let normal = axis_vector(axis) * sign;
        let (tangent_a, tangent_b) = tangents(axis);
        let extent_a = component(body_half, index_of(tangent_a)) * 0.62;
        let extent_b = component(body_half, index_of(tangent_b)) * 0.54;
        let base = normal * (component(body_half, axis) + 1.5);
        let corners = [
            base - tangent_a * extent_a - tangent_b * extent_b,
            base + tangent_a * extent_a - tangent_b * extent_b,
            base + tangent_a * extent_a + tangent_b * extent_b,
            base - tangent_a * extent_a + tangent_b * extent_b,
        ]
        .map(|local| place.point(local + body_offset));
        let view_normal = view_space(camera, place.direction(normal));
        quad(
            painter,
            camera,
            corners,
            UNIT_UVS,
            color_to_f32(
                shade(CASE_PANEL_COLOR, place.direction(normal)),
                to_byte(alpha),
            ),
            CASE_SHELL_MATERIAL_ID,
            seam,
            [view_normal.x, view_normal.y, view_normal.z, SHELL_ETCH],
        );
    }

    // The lid, hinged along the back top edge.
    let hinge = V3::new(0.0, CASE_HALF.y - LID_THICKNESS, -CASE_HALF.z + CASE_BEVEL);
    let lid_angle = stage.case_lid_angle;
    let lid_half = V3::new(CASE_HALF.x, LID_THICKNESS, CASE_HALF.z);
    let lid_center = V3::new(0.0, CASE_HALF.y - LID_THICKNESS, 0.0);
    let hinge_point = move |local: V3| {
        let swung = (local + lid_center - hinge).rotate_x(-lid_angle) + hinge;
        place.point(swung)
    };
    let hinge_direction = move |local: V3| place.direction(local.rotate_x(-lid_angle));
    push_chamfer_box(
        painter,
        camera,
        &hinge_point,
        &hinge_direction,
        lid_half,
        CASE_BEVEL * 0.7,
        CASE_LID_COLOR,
        seam,
        alpha,
    );

    // Hot seam around the lid split, brightest as the case charges.
    if seam > 0.01 {
        emit_lid_seam(painter, camera, &place, body_half, seam * alpha, session);
    }

    // Interior light, visible once the lid lifts.
    if lid_angle > 0.05 {
        let opening = (lid_angle / 2.1).clamp(0.0, 1.0);
        let corners = [
            V3::new(
                -CASE_HALF.x * 0.8,
                CASE_HALF.y - LID_THICKNESS,
                -CASE_HALF.z * 0.8,
            ),
            V3::new(
                CASE_HALF.x * 0.8,
                CASE_HALF.y - LID_THICKNESS,
                -CASE_HALF.z * 0.8,
            ),
            V3::new(
                CASE_HALF.x * 0.8,
                CASE_HALF.y - LID_THICKNESS,
                CASE_HALF.z * 0.8,
            ),
            V3::new(
                -CASE_HALF.x * 0.8,
                CASE_HALF.y - LID_THICKNESS,
                CASE_HALF.z * 0.8,
            ),
        ]
        .map(|local| place.point(local + body_offset));
        let tint = color_to_f32(SEAM_HOT, to_byte(alpha * opening));
        quad(
            painter,
            camera,
            corners,
            SPRITE_UVS,
            tint,
            CASE_GLOW_MATERIAL_ID,
            SHAPE_BOX,
            [1.6, 1.4, 0.0, 0.0],
        );
    }
}

/// A glowing band tracing the lid split on all four sides.
fn emit_lid_seam(
    painter: &mut Painter,
    camera: &Camera,
    place: &Placement,
    body_half: V3,
    strength: f32,
    session: &CaseSession,
) {
    let thickness = 7.0;
    let y = body_half.y - LID_THICKNESS + 1.0;
    let color = color_to_f32(SEAM_HOT, to_byte(strength * 0.9));
    let extra = [1.5 + strength * 2.2, 1.1, 1.0, session.stage.elapsed];
    for (axis, sign) in [(0usize, 1.0f32), (0, -1.0), (2, 1.0), (2, -1.0)] {
        let normal = axis_vector(axis) * sign;
        let along = if axis == 0 {
            V3::new(0.0, 0.0, 1.0)
        } else {
            V3::new(1.0, 0.0, 0.0)
        };
        let half_length = component(body_half, index_of(along)) - CASE_BEVEL * 0.5;
        let base = normal * (component(body_half, axis) + 2.0) + V3::new(0.0, y, 0.0);
        let corners = [
            base - along * half_length - V3::new(0.0, thickness, 0.0),
            base + along * half_length - V3::new(0.0, thickness, 0.0),
            base + along * half_length + V3::new(0.0, thickness, 0.0),
            base - along * half_length + V3::new(0.0, thickness, 0.0),
        ]
        .map(|local| place.point(local + V3::new(0.0, -LID_THICKNESS, 0.0)));
        // Band UVs put the thin axis on U (-1..1) and the run on V (0..1).
        quad(
            painter,
            camera,
            corners,
            [[-1.0, 0.0], [-1.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            color,
            CASE_GLOW_MATERIAL_ID,
            SHAPE_BAND,
            extra,
        );
    }
}

/// Emits a box with chamfered edges: six inset faces, twelve edge strips and
/// eight corner triangles. The chamfers carry the seam glow.
#[allow(clippy::too_many_arguments)]
fn push_chamfer_box(
    painter: &mut Painter,
    camera: &Camera,
    point: &dyn Fn(V3) -> V3,
    direction: &dyn Fn(V3) -> V3,
    half: V3,
    bevel: f32,
    color: AppColor,
    seam: f32,
    alpha: f32,
) {
    let bevel = bevel.min(
        component(half, 0)
            .min(component(half, 1))
            .min(component(half, 2))
            * 0.45,
    );

    // Faces.
    for axis in 0..3usize {
        for sign in [1.0f32, -1.0] {
            let normal = axis_vector(axis) * sign;
            let (tangent_a, tangent_b) = tangents(axis);
            let extent_a = component(half, index_of(tangent_a)) - bevel;
            let extent_b = component(half, index_of(tangent_b)) - bevel;
            let base = normal * component(half, axis);
            let corners = [
                base - tangent_a * extent_a - tangent_b * extent_b,
                base + tangent_a * extent_a - tangent_b * extent_b,
                base + tangent_a * extent_a + tangent_b * extent_b,
                base - tangent_a * extent_a + tangent_b * extent_b,
            ]
            .map(point);
            let world_normal = direction(normal);
            let view_normal = view_space(camera, world_normal);
            quad(
                painter,
                camera,
                corners,
                UNIT_UVS,
                color_to_f32(shade(color, world_normal), to_byte(alpha)),
                CASE_SHELL_MATERIAL_ID,
                seam,
                [view_normal.x, view_normal.y, view_normal.z, SHELL_PANEL],
            );
        }
    }

    // Chamfer strips along each of the twelve edges.
    for (axis_a, axis_b) in [(0usize, 1usize), (1, 2), (0, 2)] {
        let axis_c = 3 - axis_a - axis_b;
        for sign_a in [1.0f32, -1.0] {
            for sign_b in [1.0f32, -1.0] {
                let normal_a = axis_vector(axis_a) * sign_a;
                let normal_b = axis_vector(axis_b) * sign_b;
                let along = axis_vector(axis_c);
                let reach = component(half, axis_c) - bevel;
                let inner_a = component(half, axis_a) - bevel;
                let inner_b = component(half, axis_b) - bevel;
                let edge_a = normal_a * component(half, axis_a) + normal_b * inner_b;
                let edge_b = normal_a * inner_a + normal_b * component(half, axis_b);
                let corners = [
                    edge_a - along * reach,
                    edge_a + along * reach,
                    edge_b + along * reach,
                    edge_b - along * reach,
                ]
                .map(point);
                let world_normal = direction((normal_a + normal_b).normalized());
                let view_normal = view_space(camera, world_normal);
                quad(
                    painter,
                    camera,
                    corners,
                    UNIT_UVS,
                    color_to_f32(shade(color, world_normal), to_byte(alpha)),
                    CASE_SHELL_MATERIAL_ID,
                    seam,
                    [view_normal.x, view_normal.y, view_normal.z, SHELL_SEAM],
                );
            }
        }
    }

    // Corner facets.
    for sign_x in [1.0f32, -1.0] {
        for sign_y in [1.0f32, -1.0] {
            for sign_z in [1.0f32, -1.0] {
                let signs = V3::new(sign_x, sign_y, sign_z);
                let corners = [
                    V3::new(
                        signs.x * half.x,
                        signs.y * (half.y - bevel),
                        signs.z * (half.z - bevel),
                    ),
                    V3::new(
                        signs.x * (half.x - bevel),
                        signs.y * half.y,
                        signs.z * (half.z - bevel),
                    ),
                    V3::new(
                        signs.x * (half.x - bevel),
                        signs.y * (half.y - bevel),
                        signs.z * half.z,
                    ),
                ]
                .map(point);
                let world_normal = direction(signs.normalized());
                let view_normal = view_space(camera, world_normal);
                triangle(
                    painter,
                    camera,
                    corners,
                    [[0.5, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    color_to_f32(shade(color, world_normal), to_byte(alpha)),
                    CASE_SHELL_MATERIAL_ID,
                    seam,
                    [view_normal.x, view_normal.y, view_normal.z, SHELL_SEAM],
                );
            }
        }
    }
}

/// The column of light that erupts when the case cracks open.
fn emit_light_pillar(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    let strength = stage.case_pillar * stage.global_fade;
    if strength <= 0.004 {
        return;
    }
    let (right, _) = camera.billboard_axes();
    let base = stage.case_position;
    // Kept vertical in case space rather than fully billboarded, so it reads as
    // a beam standing in the world.
    let up = V3::new(0.0, 1.0, 0.0);
    let half_width = 130.0 + strength * 90.0;
    let height = 1500.0 * strength;
    let corners = [
        base - right * half_width,
        base + right * half_width,
        base + right * half_width + up * height,
        base - right * half_width + up * height,
    ];
    quad(
        painter,
        camera,
        corners,
        [[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]],
        color_to_f32(SEAM_HOT, to_byte(strength * 0.55)),
        CASE_GLOW_MATERIAL_ID,
        SHAPE_BAND,
        [2.2, 1.0, 1.0, stage.elapsed],
    );
}

// ---------------------------------------------------------------------------
// The reel
// ---------------------------------------------------------------------------

/// Where a card sits given its signed distance along the strip from the ticker.
fn card_placement(offset: f32, fall: f32, tilt: f32) -> Placement {
    let angle = offset / REEL_RADIUS;
    let (sin, cos) = angle.sin_cos();
    // Wrapped onto a large cylinder, so cards turn away toward the edges.
    let mut position = V3::new(REEL_RADIUS * sin, 0.0, REEL_RADIUS * cos - REEL_RADIUS);
    // Losers tumble out of frame during the reveal.
    position.y -= fall * fall * 1100.0;
    Placement {
        rotation: V3::new(tilt, angle, fall * 1.4 * tilt.signum()),
        scale: 1.0,
        position,
    }
}

fn emit_reel(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    if !stage.reel_visible || stage.reel_opacity <= 0.004 {
        return;
    }
    for (index, slot) in session.reel.iter().enumerate() {
        // The winner is drawn separately once it starts popping out.
        if index == session.winner_slot() && stage.winner_pop > 0.0 {
            continue;
        }
        let offset = index as f32 * CARD_PITCH - stage.reel_scroll;
        if offset.abs() > REEL_HALF_SPAN {
            continue;
        }
        // Cards materialise outward from the centre as the strip assembles.
        let assemble = stage.reel_assemble;
        let reach = assemble * REEL_HALF_SPAN;
        if offset.abs() > reach {
            continue;
        }
        let focus = 1.0 - (offset.abs() / REEL_HALF_SPAN).clamp(0.0, 1.0);
        let alpha = stage.reel_opacity * stage.global_fade * (0.25 + focus * 0.75);
        emit_card(
            painter,
            camera,
            session,
            slot,
            card_placement(offset, stage.debris_fall, slot.tilt),
            CardLook {
                alpha,
                dim: 0.32 + focus.powf(0.6) * 0.68,
                highlight: 0.0,
                aura: 0.0,
            },
        );
    }
}

/// Per-card shading knobs.
#[derive(Clone, Copy)]
struct CardLook {
    alpha: f32,
    /// Scales the face brightness; off-centre cards sit back.
    dim: f32,
    /// Extra punch for the winning card, 0..1.
    highlight: f32,
    /// Strength of the soft glow behind the card, 0..1.
    aura: f32,
}

fn emit_card(
    painter: &mut Painter,
    camera: &Camera,
    session: &CaseSession,
    slot: &ReelSlot,
    place: Placement,
    look: CardLook,
) {
    if look.alpha <= 0.004 {
        return;
    }
    let config = session.config();
    let tier = &config.tiers[slot.tier_index];
    let reward = &tier.rewards[slot.reward_index];
    let tier_position = if config.tiers.len() > 1 {
        tier.rank as f32 / (config.tiers.len() - 1) as f32
    } else {
        1.0
    };

    let half = V3::new(CARD_WIDTH * 0.5, CARD_HEIGHT * 0.5, CARD_DEPTH * 0.5);
    let alpha_byte = to_byte(look.alpha);

    // Soft aura behind the card, sized generously so it reads as bloom.
    if look.aura > 0.004 {
        let spread = 1.0 + look.aura * 0.7;
        let corners = [
            V3::new(
                -half.x * 2.1 * spread,
                -half.y * 1.9 * spread,
                -half.z - 6.0,
            ),
            V3::new(half.x * 2.1 * spread, -half.y * 1.9 * spread, -half.z - 6.0),
            V3::new(half.x * 2.1 * spread, half.y * 1.9 * spread, -half.z - 6.0),
            V3::new(-half.x * 2.1 * spread, half.y * 1.9 * spread, -half.z - 6.0),
        ]
        .map(|local| place.point(local));
        quad(
            painter,
            camera,
            corners,
            SPRITE_UVS,
            color_to_f32(tier.color, to_byte(look.alpha * look.aura * 0.85)),
            CASE_GLOW_MATERIAL_ID,
            SHAPE_BOX,
            [1.1 + look.aura * 1.4, 1.5, slot.seed, session.stage.elapsed],
        );
    }

    // Body: everything but the front face, flat-shaded so the front pops.
    let body_color = mix(CARD_BODY_COLOR, tier.color, 0.16);
    for axis in 0..3usize {
        for sign in [1.0f32, -1.0] {
            if axis == 2 && sign > 0.0 {
                continue; // front face is the lit material below
            }
            let normal = axis_vector(axis) * sign;
            let (tangent_a, tangent_b) = tangents(axis);
            let extent_a = component(half, index_of(tangent_a));
            let extent_b = component(half, index_of(tangent_b));
            let base = normal * component(half, axis);
            let corners = [
                base - tangent_a * extent_a - tangent_b * extent_b,
                base + tangent_a * extent_a - tangent_b * extent_b,
                base + tangent_a * extent_a + tangent_b * extent_b,
                base - tangent_a * extent_a + tangent_b * extent_b,
            ]
            .map(|local| place.point(local));
            let world_normal = place.direction(normal);
            quad(
                painter,
                camera,
                corners,
                UNIT_UVS,
                color_to_f32(
                    scale_color(shade(body_color, world_normal), look.dim),
                    alpha_byte,
                ),
                0.0,
                0.0,
                [0.0; 4],
            );
        }
    }

    // Front face, procedurally shaded by tier.
    let face_z = half.z;
    let face = [
        V3::new(-half.x, -half.y, face_z),
        V3::new(half.x, -half.y, face_z),
        V3::new(half.x, half.y, face_z),
        V3::new(-half.x, half.y, face_z),
    ]
    .map(|local| place.point(local));
    // UVs run top-left to bottom-right so the shader's gradients read upright.
    let face_uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    quad(
        painter,
        camera,
        face,
        face_uvs,
        color_to_f32(tier.color, alpha_byte),
        CASE_CARD_MATERIAL_ID,
        tier_position,
        [session.stage.elapsed, look.highlight, slot.seed, look.dim],
    );

    // Contents sit a hair in front of the face to avoid coplanar sorting.
    let content_z = face_z + 1.5;
    let text_alpha = to_byte(look.alpha * (0.55 + look.dim * 0.45));

    // Badge, filling the upper two thirds and shrunk to stay inside the card.
    let badge_room = half.x * 2.0 - 20.0;
    let badge_size = 48.0f32
        .min(badge_room / reward.badge.chars().count().max(1) as f32 * 8.0 / GLYPH_ADVANCE_CELLS);
    push_text_centered(
        painter,
        camera,
        &place,
        V3::new(0.0, half.y * 0.28, content_z),
        badge_size,
        &reward.badge,
        AppColor::from_rgb(248, 250, 255),
        text_alpha,
        0.9 + look.highlight,
    );

    // Prize name, wrapped to two lines of whatever the card can hold.
    let name_size = 15.0;
    let lines = wrap_text(
        &reward.name.to_uppercase(),
        fitting_chars(half.x * 2.0 - 18.0, name_size),
    );
    for (row, line) in lines.iter().take(2).enumerate() {
        push_text_centered(
            painter,
            camera,
            &place,
            V3::new(0.0, -half.y * 0.36 - row as f32 * 20.0, content_z),
            name_size,
            line,
            AppColor::from_rgb(226, 234, 246),
            text_alpha,
            0.5 + look.highlight * 0.5,
        );
    }

    // Rarity bar along the bottom edge, the tell CS:GO players read first.
    let bar_half_height = 7.0;
    let bar = [
        V3::new(-half.x + 8.0, -half.y + 10.0 - bar_half_height, content_z),
        V3::new(half.x - 8.0, -half.y + 10.0 - bar_half_height, content_z),
        V3::new(half.x - 8.0, -half.y + 10.0 + bar_half_height, content_z),
        V3::new(-half.x + 8.0, -half.y + 10.0 + bar_half_height, content_z),
    ]
    .map(|local| place.point(local));
    quad(
        painter,
        camera,
        bar,
        // A band, not a box: a box falls off along both axes, which leaves a
        // long thin quad looking like a dash in the middle of the card.
        [[-1.0, 0.0], [-1.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
        color_to_f32(lighten(tier.color, 0.25), to_byte(look.alpha * 0.95)),
        CASE_GLOW_MATERIAL_ID,
        SHAPE_BAND,
        [1.2 + look.highlight, 1.6, 1.0, session.stage.elapsed],
    );
}

/// The blade the winning card lands under.
fn emit_ticker(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    if !stage.reel_visible || stage.reel_opacity <= 0.004 {
        return;
    }
    let alpha = stage.reel_opacity * stage.global_fade;
    let flash = stage.ticker_flash;
    let reach = CARD_HEIGHT * 0.5 + 46.0;
    let half_width = 3.5 + flash * 3.0;
    let z = CARD_DEPTH * 0.5 + 30.0;

    // Blade.
    let blade = [
        V3::new(-half_width, -reach, z),
        V3::new(half_width, -reach, z),
        V3::new(half_width, reach, z),
        V3::new(-half_width, reach, z),
    ];
    quad(
        painter,
        camera,
        blade,
        [[-1.0, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]],
        color_to_f32(TICKER_COLOR, to_byte(alpha * (0.55 + flash * 0.45))),
        CASE_GLOW_MATERIAL_ID,
        SHAPE_BAND,
        [1.8 + flash * 3.0, 1.0, 1.0, stage.elapsed],
    );

    // Chevrons pointing in from above and below.
    for sign in [1.0f32, -1.0] {
        let tip = V3::new(0.0, sign * (reach - 20.0), z);
        let base_y = sign * (reach + 20.0);
        let corners = [tip, V3::new(-22.0, base_y, z), V3::new(22.0, base_y, z)];
        triangle(
            painter,
            camera,
            corners,
            [[0.5, 0.0], [0.0, 1.0], [1.0, 1.0]],
            color_to_f32(TICKER_COLOR, to_byte(alpha * (0.7 + flash * 0.3))),
            0.0,
            0.0,
            [0.0; 4],
        );
    }
}

// ---------------------------------------------------------------------------
// Reveal
// ---------------------------------------------------------------------------

/// Where the winning card sits during and after the reveal.
fn winner_placement(session: &CaseSession) -> Placement {
    let stage = &session.stage;
    let offset = session.winner_slot() as f32 * CARD_PITCH - stage.reel_scroll;
    let lift = stage.winner_lift;
    let mut place = card_placement(offset * (1.0 - lift), 0.0, 0.0);
    let reel_rotation = place.rotation;
    // Ride toward the camera and settle dead centre.
    place.position = place.position.lerp(V3::new(0.0, 0.0, 260.0 * lift), lift);
    place.scale = 1.0 + stage.winner_pop * 0.25;
    if stage.phase == CasePhase::Wheel {
        // Rises out of frame while it fades. The simulation drives the fade, so
        // reading the exit back off it keeps the two in step.
        let exit = 1.0 - stage.winner_pop;
        place.position.y += exit * 560.0;
        place.scale *= 1.0 - exit * 0.4;
    }
    // Unwind out of the reel's cylinder turn, then keep a slow sway going so
    // the hold never looks frozen.
    let sway = (stage.elapsed * 1.1).sin() * 0.09 * stage.winner_pop;
    place.rotation = V3::new(
        reel_rotation.x * (1.0 - lift) + sway * 0.4,
        reel_rotation.y * (1.0 - lift) + sway,
        reel_rotation.z * (1.0 - lift),
    );
    place
}

fn emit_winner(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    if stage.winner_pop <= 0.004 {
        return;
    }
    let place = winner_placement(session);
    let slot = session.reel[session.winner_slot()];
    let intensity = session.intensity();

    emit_rays(painter, camera, session, &place);

    emit_card(
        painter,
        camera,
        session,
        &slot,
        place,
        CardLook {
            alpha: stage.global_fade * stage.winner_pop.min(1.0),
            dim: 1.0,
            highlight: stage.winner_pop.min(1.0),
            aura: (0.5 + intensity * 0.5) * stage.winner_pop.min(1.0),
        },
    );

    emit_result_text(painter, camera, session, &place);
}

/// God rays fanning out from behind the prize.
fn emit_rays(painter: &mut Painter, camera: &Camera, session: &CaseSession, place: &Placement) {
    let stage = &session.stage;
    let strength = stage.rays * stage.global_fade;
    if strength <= 0.004 {
        return;
    }
    let tier_color = session.tier_color();
    let count = 14;
    let spin = stage.elapsed * 0.35;
    let origin = place.position - V3::new(0.0, 0.0, 40.0);
    let (right, up) = camera.billboard_axes();
    for index in 0..count {
        let base_angle = index as f32 / count as f32 * std::f32::consts::TAU + spin;
        // Vary the length per ray so the fan isn't a perfect star.
        let wobble = ((index as f32 * 2.7) + stage.elapsed * 1.3).sin() * 0.5 + 0.5;
        let length = 520.0 + wobble * 380.0;
        let spread = 0.055 + wobble * 0.03;
        let direction = |angle: f32| right * angle.cos() + up * angle.sin();
        let corners = [
            origin + direction(base_angle) * 90.0,
            origin + direction(base_angle - spread) * length,
            origin + direction(base_angle + spread) * length,
        ];
        triangle(
            painter,
            camera,
            corners,
            [[0.0, 0.0], [1.0, -1.0], [1.0, 1.0]],
            color_to_f32(tier_color, to_byte(strength * (0.28 + wobble * 0.3))),
            CASE_RAY_MATERIAL_ID,
            1.6,
            [1.4, 0.0, index as f32, stage.elapsed],
        );
    }
}

fn emit_result_text(
    painter: &mut Painter,
    camera: &Camera,
    session: &CaseSession,
    place: &Placement,
) {
    let stage = &session.stage;
    let reveal = stage.result_reveal;
    if reveal <= 0.004 {
        return;
    }
    let alpha = to_byte(reveal * stage.global_fade);
    let tier_color = session.tier_color();
    // Anchored to the card so the block tracks it as it scales and sways.
    let top = -CARD_HEIGHT * 0.5 - 34.0;
    // Slide up into place as it fades in.
    let slide = (1.0 - reveal) * 40.0;

    push_text_centered(
        painter,
        camera,
        place,
        V3::new(0.0, top - slide, 0.0),
        22.0,
        &session.tier_line(),
        lighten(tier_color, 0.3),
        alpha,
        1.0,
    );
    push_text_centered(
        painter,
        camera,
        place,
        V3::new(0.0, top - 52.0 - slide, 0.0),
        40.0,
        &session.reward_name(),
        AppColor::from_rgb(255, 255, 255),
        alpha,
        1.5,
    );
    let description = session.reward_description();
    if !description.is_empty() {
        for (row, line) in wrap_text(&description.to_uppercase(), 42)
            .iter()
            .take(2)
            .enumerate()
        {
            push_text_centered(
                painter,
                camera,
                place,
                V3::new(0.0, top - 100.0 - row as f32 * 24.0 - slide, 0.0),
                17.0,
                line,
                AppColor::from_rgb(186, 200, 220),
                alpha,
                0.4,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Legendary wheel
// ---------------------------------------------------------------------------

fn emit_wheel(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    if !stage.wheel_visible || stage.wheel_scale <= 0.004 {
        return;
    }
    let Some(wheel) = session.config().wheel.as_ref() else {
        return;
    };
    if wheel.prizes.is_empty() {
        return;
    }
    let tier_color = session.tier_color();
    let alpha = stage.global_fade;
    let scale = stage.wheel_scale;
    let center = WHEEL_CENTER;
    let slot_angle = std::f32::consts::TAU / wheel.prizes.len() as f32;
    let winner = session.wheel_prize_index().unwrap_or(0);
    // Layout comes from the simulation, which needs it to place the landing
    // celebration on the winning slot.
    let radius = wheel_radius(wheel.prizes.len());

    let panel_half = WHEEL_PANEL_HALF;
    for (index, prize) in wheel.prizes.iter().enumerate() {
        let azimuth = index as f32 * slot_angle - stage.wheel_angle;
        let (sin, cos) = azimuth.sin_cos();
        let position = center + V3::new(radius * sin, 0.0, radius * cos) * scale;
        let place = Placement {
            rotation: V3::new(0.0, azimuth, 0.0),
            scale,
            position,
        };
        // Panels facing away from the eye dim down, and show only their slab:
        // a face drawn from behind winds backwards and reads as mirrored text.
        let facing = ((cos + 1.0) * 0.5).powf(1.4);
        let front = cos > 0.05;
        let is_winner = index == winner;
        let won = if is_winner { stage.wheel_reveal } else { 0.0 };
        let panel_alpha = alpha * (0.35 + facing * 0.65);

        // Panel slab.
        let color = mix(WHEEL_PANEL_COLOR, tier_color, 0.18 + won * 0.5);
        for axis in 0..3usize {
            for sign in [1.0f32, -1.0] {
                if axis == 2 && sign > 0.0 {
                    continue;
                }
                let normal = axis_vector(axis) * sign;
                let (tangent_a, tangent_b) = tangents(axis);
                let base = normal * component(panel_half, axis);
                let corners = [
                    base - tangent_a * component(panel_half, index_of(tangent_a))
                        - tangent_b * component(panel_half, index_of(tangent_b)),
                    base + tangent_a * component(panel_half, index_of(tangent_a))
                        - tangent_b * component(panel_half, index_of(tangent_b)),
                    base + tangent_a * component(panel_half, index_of(tangent_a))
                        + tangent_b * component(panel_half, index_of(tangent_b)),
                    base - tangent_a * component(panel_half, index_of(tangent_a))
                        + tangent_b * component(panel_half, index_of(tangent_b)),
                ]
                .map(|local| place.point(local));
                let world_normal = place.direction(normal);
                quad(
                    painter,
                    camera,
                    corners,
                    UNIT_UVS,
                    color_to_f32(shade(color, world_normal), to_byte(panel_alpha)),
                    0.0,
                    0.0,
                    [0.0; 4],
                );
            }
        }

        if !front {
            continue;
        }

        // Face and prize name.
        let face = [
            V3::new(-panel_half.x, -panel_half.y, panel_half.z),
            V3::new(panel_half.x, -panel_half.y, panel_half.z),
            V3::new(panel_half.x, panel_half.y, panel_half.z),
            V3::new(-panel_half.x, panel_half.y, panel_half.z),
        ]
        .map(|local| place.point(local));
        quad(
            painter,
            camera,
            face,
            [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            color_to_f32(tier_color, to_byte(panel_alpha)),
            CASE_CARD_MATERIAL_ID,
            1.0,
            [stage.elapsed, won, index as f32 * 0.17, 0.5 + facing * 0.5],
        );

        let prize_size = 22.0;
        let prize_lines = wrap_text(
            &prize.name.to_uppercase(),
            fitting_chars(panel_half.x * 2.0 - 28.0, prize_size),
        );
        for (row, line) in prize_lines.iter().take(2).enumerate() {
            push_text_centered(
                painter,
                camera,
                &place,
                V3::new(0.0, 16.0 - row as f32 * 28.0, panel_half.z + 2.0),
                prize_size,
                line,
                AppColor::from_rgb(255, 252, 240),
                to_byte(panel_alpha),
                0.6 + won,
            );
        }

        if won > 0.01 {
            let corners = [
                V3::new(-panel_half.x * 2.2, -panel_half.y * 2.4, -12.0),
                V3::new(panel_half.x * 2.2, -panel_half.y * 2.4, -12.0),
                V3::new(panel_half.x * 2.2, panel_half.y * 2.4, -12.0),
                V3::new(-panel_half.x * 2.2, panel_half.y * 2.4, -12.0),
            ]
            .map(|local| place.point(local));
            quad(
                painter,
                camera,
                corners,
                SPRITE_UVS,
                color_to_f32(tier_color, to_byte(alpha * won * stage.wheel_glow * 0.9)),
                CASE_GLOW_MATERIAL_ID,
                SHAPE_BOX,
                [2.0, 1.4, 0.0, stage.elapsed],
            );
        }
    }

    // Pointer at the front of the carousel.
    let pointer_place =
        Placement::new(center + V3::new(0.0, (panel_half.y + 58.0) * scale, radius * scale));
    let corners = [
        V3::new(0.0, -34.0, 0.0),
        V3::new(-26.0, 26.0, 0.0),
        V3::new(26.0, 26.0, 0.0),
    ]
    .map(|local| pointer_place.point(local * scale));
    triangle(
        painter,
        camera,
        corners,
        [[0.5, 1.0], [0.0, 0.0], [1.0, 0.0]],
        color_to_f32(TICKER_COLOR, to_byte(alpha * 0.95)),
        0.0,
        0.0,
        [0.0; 4],
    );
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

fn emit_shockwaves(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let fade = session.stage.global_fade;
    let (right, up) = camera.billboard_axes();
    for wave in &session.particles.shockwaves {
        let remaining = wave.remaining();
        // Rings brighten on birth then thin out.
        let strength = remaining * remaining * fade;
        if strength <= 0.004 {
            continue;
        }
        let inner = (wave.radius - wave.thickness).max(0.0);
        let outer = wave.radius + wave.thickness;
        let segments = 40;
        let color = color_to_f32(wave.color, to_byte(strength * 0.8));
        // Taper is off: the ring is 40 segments, and tapering each one would
        // punch gaps all the way around.
        let extra = [1.6, 1.0, 0.0, session.stage.elapsed];
        for segment in 0..segments {
            let a = segment as f32 / segments as f32 * std::f32::consts::TAU + wave.spin;
            let b = (segment + 1) as f32 / segments as f32 * std::f32::consts::TAU + wave.spin;
            // Tilt turns the ring from a flat disc into a receding hoop.
            let point = |angle: f32, radius: f32| {
                let flat = right * (angle.cos() * radius) + up * (angle.sin() * radius);
                wave.center
                    + V3::new(
                        flat.x,
                        flat.y * wave.tilt.cos(),
                        flat.z + flat.y * wave.tilt.sin(),
                    )
            };
            quad(
                painter,
                camera,
                [
                    point(a, inner),
                    point(b, inner),
                    point(b, outer),
                    point(a, outer),
                ],
                [[-1.0, 0.0], [-1.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
                color,
                CASE_GLOW_MATERIAL_ID,
                SHAPE_BAND,
                extra,
            );
        }
    }
}

fn emit_particles(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    let fade = stage.global_fade;
    let (right, up) = camera.billboard_axes();

    for particle in &session.particles.particles {
        let alpha = particle.fade() * fade;
        if alpha <= 0.004 {
            continue;
        }
        match particle.kind {
            ParticleKind::Confetti | ParticleKind::Shard => {
                // Solid tumbling geometry, lit on the CPU.
                let axis = particle.tumble_axis.normalized();
                let base_x = axis.cross(V3::new(0.0, 1.0, 0.0)).normalized();
                let base_y = axis.cross(base_x).normalized();
                let (sin, cos) = particle.spin.sin_cos();
                let edge_x = (base_x * cos + base_y * sin) * particle.size;
                let edge_y = axis * (particle.size * 0.55);
                let corners = [
                    particle.position - edge_x - edge_y,
                    particle.position + edge_x - edge_y,
                    particle.position + edge_x + edge_y,
                    particle.position - edge_x + edge_y,
                ];
                let normal = edge_x.cross(edge_y).normalized();
                quad(
                    painter,
                    camera,
                    corners,
                    UNIT_UVS,
                    color_to_f32(shade(particle.color, normal), to_byte(alpha)),
                    0.0,
                    0.0,
                    [0.0; 4],
                );
            }
            ParticleKind::Streak => {
                // Stretched along travel, so speed lines actually point.
                let direction = particle.velocity.normalized();
                let length = particle.size * 14.0;
                let across = direction.cross(camera.forward).normalized() * particle.size;
                let corners = [
                    particle.position - direction * length - across,
                    particle.position + direction * length - across,
                    particle.position + direction * length + across,
                    particle.position - direction * length + across,
                ];
                quad(
                    painter,
                    camera,
                    corners,
                    SPRITE_UVS,
                    color_to_f32(particle.color, to_byte(alpha * 0.7)),
                    CASE_GLOW_MATERIAL_ID,
                    SHAPE_SPRITE,
                    [particle.glow, 1.6, particle.seed, stage.elapsed],
                );
            }
            _ => {
                let size = particle.size * particle_size_scale(particle.kind, particle.age());
                let edge_x = right * size;
                let edge_y = up * size;
                let corners = [
                    particle.position - edge_x - edge_y,
                    particle.position + edge_x - edge_y,
                    particle.position + edge_x + edge_y,
                    particle.position - edge_x + edge_y,
                ];
                quad(
                    painter,
                    camera,
                    corners,
                    SPRITE_UVS,
                    color_to_f32(particle.color, to_byte(alpha)),
                    CASE_GLOW_MATERIAL_ID,
                    SHAPE_SPRITE,
                    [
                        particle.glow,
                        particle_falloff(particle.kind),
                        particle.seed,
                        stage.elapsed,
                    ],
                );
            }
        }
    }
}

/// Sparks stretch a little as they age; embers swell.
fn particle_size_scale(kind: ParticleKind, age: f32) -> f32 {
    match kind {
        ParticleKind::Spark => 1.0 + age * 0.6,
        ParticleKind::Ember => 1.0 + age * 1.4,
        _ => 1.0,
    }
}

fn particle_falloff(kind: ParticleKind) -> f32 {
    match kind {
        ParticleKind::Spark => 2.4,
        ParticleKind::Ember => 1.5,
        ParticleKind::Dust => 1.1,
        _ => 1.8,
    }
}

// ---------------------------------------------------------------------------
// Headings
// ---------------------------------------------------------------------------

fn emit_headings(painter: &mut Painter, camera: &Camera, session: &CaseSession) {
    let stage = &session.stage;
    let fade = stage.global_fade;
    if fade <= 0.004 {
        return;
    }
    // Titles ride in on the intro and stay put after.
    let entry = if stage.phase == CasePhase::Intro {
        smooth(stage.phase_progress * 1.4)
    } else {
        1.0
    };
    let alpha = to_byte(fade * entry);
    let slide = (1.0 - entry) * 90.0;
    let place = Placement::new(V3::new(0.0, 0.0, 120.0));

    push_text_centered(
        painter,
        camera,
        &place,
        V3::new(0.0, 402.0 + slide, 0.0),
        20.0,
        &session.header_line(),
        AppColor::from_rgb(168, 190, 214),
        alpha,
        0.35,
    );
    push_text_centered(
        painter,
        camera,
        &place,
        V3::new(0.0, 356.0 + slide, 0.0),
        36.0,
        &session.case_title(),
        AppColor::from_rgb(240, 246, 255),
        alpha,
        1.1,
    );
    push_text_centered(
        painter,
        camera,
        &place,
        V3::new(0.0, 312.0 + slide, 0.0),
        17.0,
        &session.status_line(),
        lighten(session.tier_color(), 0.45),
        to_byte(fade * entry * 0.85),
        0.6,
    );
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Emits an 8x8 bitmap string as one quad per glyph. The pixel shader
/// reconstructs and anti-aliases the glyph from a packed bitmask, so text stays
/// crisp at any size instead of turning into visible 8x8 blocks.
#[allow(clippy::too_many_arguments)]
fn push_text(
    painter: &mut Painter,
    camera: &Camera,
    place: &Placement,
    origin: V3,
    glyph_size: f32,
    text: &str,
    color: AppColor,
    alpha: u8,
    weight: f32,
) {
    if alpha == 0 || text.is_empty() {
        return;
    }
    let cell = glyph_size / 8.0;
    let advance = glyph_advance(glyph_size);
    let color = color_to_f32(color, alpha);
    // One sort key for the whole string, taken from its anchor and biased
    // forward. Per-glyph keys would let the far end of a string on an angled
    // surface sort behind that surface and get painted over.
    let Some(anchor) = camera.project(place.point(origin)) else {
        return;
    };
    let sort_depth = anchor.depth - TEXT_SORT_BIAS;
    let mut cursor = origin.x;
    for character in text.chars() {
        let glyph = match font8x8::BASIC_FONTS.get(character) {
            Some(glyph) => glyph,
            None => {
                cursor += advance;
                continue;
            }
        };
        if character != ' ' {
            let rows = pack_glyph(&glyph);
            // A margin around the cell grid leaves room for the outline.
            let margin = 1.0;
            let min = V3::new(cursor - margin * cell, origin.y - margin * cell, origin.z);
            let max = V3::new(
                cursor + (8.0 + margin) * cell,
                origin.y + (8.0 + margin) * cell,
                origin.z,
            );
            let corners = [
                V3::new(min.x, min.y, min.z),
                V3::new(max.x, min.y, min.z),
                V3::new(max.x, max.y, min.z),
                V3::new(min.x, max.y, min.z),
            ]
            .map(|local| place.point(local));
            // Glyph-cell coordinates, flipped so row 0 is the glyph's top.
            let cells = [
                [-margin, 8.0 + margin],
                [8.0 + margin, 8.0 + margin],
                [8.0 + margin, -margin],
                [-margin, -margin],
            ];
            let mut projected = [None; 4];
            for (slot, corner) in projected.iter_mut().zip(corners.iter()) {
                *slot = camera.project(*corner);
            }
            if let Some(points) = collect4(projected) {
                let build = |index: usize| GpuVertex {
                    position: [points[index].x, points[index].y, CASE_Z],
                    color,
                    material: [
                        CASE_GLYPH_MATERIAL_ID,
                        rows[0] as f32,
                        rows[1] as f32,
                        rows[2] as f32,
                    ],
                    material_extra: [
                        rows[3] as f32,
                        cells[index][0],
                        cells[index][1],
                        weight.clamp(0.0, 4.0),
                    ],
                };
                painter.triangle(build(0), build(1), build(2), sort_depth);
                painter.triangle(build(0), build(2), build(3), sort_depth);
            }
        }
        cursor += advance;
    }
}

fn glyph_advance(glyph_size: f32) -> f32 {
    glyph_size / 8.0 * GLYPH_ADVANCE_CELLS
}

/// How wide a string prints. Used for centring, and for choosing how many
/// characters fit across a card.
fn text_width(text: &str, glyph_size: f32) -> f32 {
    text.chars().count() as f32 * glyph_advance(glyph_size)
}

/// The most characters that fit in `width`, for wrapping.
fn fitting_chars(width: f32, glyph_size: f32) -> usize {
    (width / glyph_advance(glyph_size)).floor().max(1.0) as usize
}

#[allow(clippy::too_many_arguments)]
fn push_text_centered(
    painter: &mut Painter,
    camera: &Camera,
    place: &Placement,
    center: V3,
    glyph_size: f32,
    text: &str,
    color: AppColor,
    alpha: u8,
    weight: f32,
) {
    let width = text_width(text, glyph_size);
    push_text(
        painter,
        camera,
        place,
        V3::new(
            center.x - width * 0.5,
            center.y - glyph_size * 0.5,
            center.z,
        ),
        glyph_size,
        text,
        color,
        alpha,
        weight,
    );
}

/// Packs an 8x8 glyph into four floats, two rows each, matching the layout the
/// chat glyph shader already uses.
fn pack_glyph(glyph: &[u8; 8]) -> [u16; 4] {
    let mut packed = [0u16; 4];
    for (row_index, row) in glyph.iter().enumerate() {
        let slot = row_index / 2;
        let shift = (row_index % 2) * 8;
        packed[slot] |= (*row as u16) << shift;
    }
    packed
}

/// Greedy word wrap. Long single words are hard-split so a card never
/// overflows.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word = word;
        while word.chars().count() > max_chars {
            let head: String = word.chars().take(max_chars).collect();
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(head);
            let skip: usize = word
                .char_indices()
                .nth(max_chars)
                .map(|(index, _)| index)
                .unwrap_or(word.len());
            word = &word[skip..];
        }
        if word.is_empty() {
            continue;
        }
        let projected = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if projected > max_chars && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

const CASE_BODY_COLOR: AppColor = AppColor::from_rgb(74, 84, 100);
const CASE_LID_COLOR: AppColor = AppColor::from_rgb(88, 99, 118);
const CASE_PANEL_COLOR: AppColor = AppColor::from_rgb(52, 60, 74);
const CARD_BODY_COLOR: AppColor = AppColor::from_rgb(26, 31, 40);
const WHEEL_PANEL_COLOR: AppColor = AppColor::from_rgb(34, 38, 50);
const SEAM_HOT: AppColor = AppColor::from_rgb(255, 212, 130);
const TICKER_COLOR: AppColor = AppColor::from_rgb(255, 244, 214);

fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn axis_vector(axis: usize) -> V3 {
    match axis {
        0 => V3::new(1.0, 0.0, 0.0),
        1 => V3::new(0.0, 1.0, 0.0),
        _ => V3::new(0.0, 0.0, 1.0),
    }
}

/// The two axes perpendicular to `axis`, in a consistent order.
fn tangents(axis: usize) -> (V3, V3) {
    match axis {
        0 => (axis_vector(2), axis_vector(1)),
        1 => (axis_vector(0), axis_vector(2)),
        _ => (axis_vector(0), axis_vector(1)),
    }
}

fn index_of(axis: V3) -> usize {
    if axis.x != 0.0 {
        0
    } else if axis.y != 0.0 {
        1
    } else {
        2
    }
}

fn component(vector: V3, axis: usize) -> f32 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn view_space(camera: &Camera, direction: V3) -> V3 {
    V3::new(
        direction.dot(camera.right),
        direction.dot(camera.up),
        direction.dot(camera.forward),
    )
}

/// CPU lighting for solid surfaces. Case space has Y up, so this cannot reuse
/// the overlay's screen-space light rig.
fn shade(color: AppColor, normal: V3) -> AppColor {
    let key = V3::new(-0.42, 0.76, 0.50).normalized();
    let fill = V3::new(0.65, -0.20, 0.42).normalized();
    let key_amount = normal.dot(key).max(0.0);
    let fill_amount = normal.dot(fill).max(0.0);
    // Rim brightens surfaces turning away from the eye.
    let rim = (1.0 - normal.z.abs()).max(0.0).powf(2.0);
    let level = 0.34 + key_amount * 0.72 + fill_amount * 0.20 + rim * 0.14;
    scale_color(color, level)
}

fn scale_color(color: AppColor, level: f32) -> AppColor {
    let apply = |channel: u8| ((channel as f32 * level).clamp(0.0, 255.0)) as u8;
    AppColor::from_argb(color.a, apply(color.r), apply(color.g), apply(color.b))
}

fn lighten(color: AppColor, amount: f32) -> AppColor {
    let apply = |channel: u8| {
        let value = channel as f32 / 255.0;
        (((value + (1.0 - value) * amount) * 255.0).clamp(0.0, 255.0)) as u8
    };
    AppColor::from_argb(color.a, apply(color.r), apply(color.g), apply(color.b))
}

fn mix(from: AppColor, to: AppColor, amount: f32) -> AppColor {
    let blend = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount).clamp(0.0, 255.0) as u8;
    AppColor::from_argb(
        from.a,
        blend(from.r, to.r),
        blend(from.g, to.g),
        blend(from.b, to.b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use case_sim::{CaseConfig, CaseRequest, CaseSession};

    /// A 1080p stage at the origin, as a single-monitor overlay would give.
    fn stage() -> RectF {
        RectF::new(0.0, 0.0, 1920.0, 1080.0)
    }

    fn session_for(tier: &str) -> CaseSession {
        CaseSession::new(
            CaseConfig::default(),
            CaseRequest {
                viewer: "Tester".to_string(),
                forced_tier: Some(tier.to_string()),
                seed: Some(99),
                streak: 1,
                ..CaseRequest::default()
            },
        )
    }

    #[test]
    fn wrapping_splits_on_words() {
        assert_eq!(
            wrap_text("PLAY WITH STREAMER", 13),
            vec!["PLAY WITH", "STREAMER"]
        );
        assert_eq!(wrap_text("VIP", 13), vec!["VIP"]);
        assert!(wrap_text("", 13).is_empty());
    }

    #[test]
    fn wrapping_hard_splits_a_word_that_cannot_fit() {
        let lines = wrap_text("SUPERCALIFRAGILISTIC", 8);
        assert!(lines.iter().all(|line| line.chars().count() <= 8));
        assert_eq!(lines.concat(), "SUPERCALIFRAGILISTIC");
    }

    #[test]
    fn glyph_packing_round_trips_every_bit() {
        let glyph: [u8; 8] = [0b1010_1010, 0xFF, 0x00, 0x01, 0x80, 0x3C, 0x7E, 0x18];
        let packed = pack_glyph(&glyph);
        for (row_index, expected) in glyph.iter().enumerate() {
            let slot = packed[row_index / 2];
            let shift = (row_index % 2) * 8;
            assert_eq!(((slot >> shift) & 0xFF) as u8, *expected, "row {row_index}");
        }
    }

    #[test]
    fn the_camera_puts_the_origin_at_screen_centre() {
        let camera = Camera::new(&CameraRig::default(), stage());
        let projected = camera.project(V3::ZERO).expect("origin should be visible");
        assert!((projected.x - 960.0).abs() < 1.0);
        // The default rig looks slightly down from above, so Y sits just off centre.
        assert!((projected.y - 540.0).abs() < 30.0);
    }

    #[test]
    fn an_offset_stage_centres_on_that_monitor() {
        // The right-hand screen of a two-monitor overlay.
        let right_screen = RectF::new(1920.0, 0.0, 1920.0, 1080.0);
        let camera = Camera::new(&CameraRig::default(), right_screen);
        let projected = camera.project(V3::ZERO).expect("origin should be visible");
        assert!((projected.x - 2880.0).abs() < 1.0);

        let mut session = session_for("covert");
        for _ in 0..240 {
            session.update(1.0 / 60.0);
        }
        let mut out = Vec::new();
        emit_case_opening(&mut out, right_screen, &session);
        let mean = out.iter().map(|vertex| vertex.position[0]).sum::<f32>() / out.len() as f32;
        assert!(
            mean > 1920.0,
            "geometry averaged x={mean:.0}, which spills onto the left screen"
        );
    }

    #[test]
    fn the_camera_drops_points_behind_the_eye() {
        let camera = Camera::new(&CameraRig::default(), stage());
        assert!(camera.project(V3::new(0.0, 0.0, 4000.0)).is_none());
    }

    #[test]
    fn projection_keeps_right_and_up_oriented_correctly() {
        let camera = Camera::new(&CameraRig::default(), stage());
        let center = camera.project(V3::ZERO).unwrap();
        let right = camera.project(V3::new(200.0, 0.0, 0.0)).unwrap();
        let above = camera.project(V3::new(0.0, 200.0, 0.0)).unwrap();
        assert!(right.x > center.x, "+X should move right on screen");
        assert!(
            above.y < center.y,
            "+Y should move up, which is -Y on screen"
        );
    }

    #[test]
    fn painter_replays_triangles_far_to_near() {
        let mut painter = Painter::default();
        let make = |tag: f32| GpuVertex {
            position: [tag, 0.0, CASE_Z],
            color: [1.0; 4],
            material: [0.0; 4],
            material_extra: [0.0; 4],
        };
        painter.triangle(make(1.0), make(1.0), make(1.0), 100.0);
        painter.triangle(make(2.0), make(2.0), make(2.0), 900.0);
        painter.triangle(make(3.0), make(3.0), make(3.0), 500.0);
        let mut out = Vec::new();
        painter.flush(&mut out);
        assert_eq!(out.len(), 9);
        assert_eq!(out[0].position[0], 2.0);
        assert_eq!(out[3].position[0], 3.0);
        assert_eq!(out[6].position[0], 1.0);
    }

    #[test]
    fn every_phase_emits_well_formed_geometry() {
        for tier in ["mil_spec", "covert", "rare_special"] {
            let mut session = session_for(tier);
            let mut peak = 0usize;
            let mut ticks = 0;
            while !session.is_finished() && ticks < 8000 {
                session.update(1.0 / 60.0);
                ticks += 1;
                let mut out = Vec::new();
                emit_case_opening(&mut out, stage(), &session);
                assert_eq!(out.len() % 3, 0, "{tier} emitted a partial triangle");
                for vertex in &out {
                    assert!(
                        vertex.position.iter().all(|value| value.is_finite()),
                        "{tier} emitted a non-finite position"
                    );
                    assert!(
                        vertex.color.iter().all(|value| value.is_finite()),
                        "{tier} emitted a non-finite colour"
                    );
                    assert_eq!(vertex.position[2], CASE_Z);
                }
                peak = peak.max(out.len());
            }
            assert!(peak > 0, "{tier} never emitted anything");
            // Headroom check: a frame that blows past this would stall the
            // single non-instanced draw call the overlay uses.
            assert!(
                peak < 90_000,
                "{tier} peaked at {peak} vertices, which is too heavy"
            );
        }
    }

    #[test]
    fn a_finished_session_emits_nothing() {
        let mut session = session_for("mil_spec");
        for _ in 0..8000 {
            session.update(1.0 / 60.0);
            if session.is_finished() {
                break;
            }
        }
        let mut out = Vec::new();
        emit_case_opening(&mut out, stage(), &session);
        assert!(out.is_empty());
    }

    #[test]
    fn the_reel_culls_cards_outside_the_window() {
        let mut session = session_for("mil_spec");
        // Part way through the spin, only a handful of cards can be on screen.
        for _ in 0..(60.0 * 5.0) as usize {
            session.update(1.0 / 60.0);
        }
        let visible = session
            .reel
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                let offset = *index as f32 * CARD_PITCH - session.stage.reel_scroll;
                offset.abs() <= REEL_HALF_SPAN
            })
            .count();
        assert!(visible > 0 && visible < 12, "{visible} cards in the window");
    }

    #[test]
    fn tiny_viewports_do_not_panic() {
        let mut session = session_for("covert");
        for _ in 0..600 {
            session.update(1.0 / 60.0);
            let mut out = Vec::new();
            emit_case_opening(&mut out, RectF::new(0.0, 0.0, 1.0, 1.0), &session);
            for vertex in &out {
                assert!(vertex.position.iter().all(|value| value.is_finite()));
            }
        }
    }
}
