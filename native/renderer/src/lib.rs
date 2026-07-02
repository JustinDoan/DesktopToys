use std::{collections::HashMap, fs::File, io::BufReader, mem, path::Path, sync::OnceLock};

use anyhow::{bail, Context, Result};
use core_types::{AppColor, ObjectState, ObjectVisualKind, RectF, Vector2};
use font8x8::UnicodeFonts;
use gltf::{
    animation::util::ReadOutputs,
    mesh::util::{ReadIndices, ReadJoints, ReadWeights},
};

#[derive(Clone, Debug)]
pub struct PanelLine {
    pub text: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct OverlayPanel {
    pub title: String,
    pub lines: Vec<PanelLine>,
    pub footer: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct HudState {
    pub status_message: Option<String>,
    pub panels: Vec<OverlayPanel>,
}

pub struct RenderScene<'a> {
    pub bounds: RectF,
    pub elapsed_seconds: f64,
    pub objects: &'a [ObjectState],
    pub cursor: Vector2,
    pub hud: &'a HudState,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Default)]
pub struct SceneRenderer {
    generated_models: HashMap<MeshCacheKey, Mesh>,
    imported_models: HashMap<String, Mesh>,
    vertices: Vec<GpuVertex>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct MeshCacheKey {
    kind: u8,
    width_milli: i32,
    height_milli: i32,
    size_milli: i32,
    color: AppColor,
}

#[derive(Clone, Debug)]
struct Mesh {
    triangles: Vec<SourceTriangle>,
}

#[derive(Clone, Copy, Debug)]
struct SourceTriangle {
    vertices: [Vec3; 3],
    color: AppColor,
    alpha: u8,
}

#[derive(Clone, Debug)]
struct FoxAsset {
    vertices: Vec<FoxVertex>,
    indices: Vec<[usize; 3]>,
    color: AppColor,
    nodes: Vec<FoxNode>,
    skin_joints: Vec<usize>,
    inverse_bind_matrices: Vec<Mat4>,
    animations: Vec<FoxAnimation>,
}

#[derive(Clone, Copy, Debug)]
struct FoxVertex {
    position: Vec3,
    joints: [usize; 4],
    weights: [f32; 4],
}

#[derive(Clone, Debug)]
struct FoxNode {
    children: Vec<usize>,
    base_translation: Vec3,
    base_rotation: Quat,
    base_scale: Vec3,
}

#[derive(Clone, Debug)]
struct FoxAnimation {
    name: String,
    duration: f32,
    channels: Vec<FoxAnimationChannel>,
}

#[derive(Clone, Debug)]
struct FoxAnimationChannel {
    node_index: usize,
    property: FoxAnimatedProperty,
    times: Vec<f32>,
    values: FoxAnimationValues,
}

#[derive(Clone, Copy, Debug)]
enum FoxAnimatedProperty {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Debug)]
enum FoxAnimationValues {
    Vec3(Vec<Vec3>),
    Rotation(Vec<Quat>),
}

#[derive(Clone, Copy, Debug)]
struct Mat4 {
    values: [[f32; 4]; 4],
}

#[derive(Clone, Copy, Debug)]
struct Quat {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

#[derive(Clone, Copy, Debug)]
struct DrawTriangle {
    points: [Vec3; 3],
    color: AppColor,
    alpha: u8,
}

const MAX_IMPORTED_TRIANGLES: usize = 4_000;

#[derive(Clone, Copy, Debug, Default)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn dot(self, rhs: Self) -> f32 {
        (self.x * rhs.x) + (self.y * rhs.y) + (self.z * rhs.z)
    }

    fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    fn normalized(self) -> Self {
        let length = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        if length <= f32::EPSILON {
            return Self::new(0.0, 0.0, 1.0);
        }
        Self::new(self.x / length, self.y / length, self.z / length)
    }
}

impl Mat4 {
    fn identity() -> Self {
        Self {
            values: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    fn from_gltf_matrix(matrix: [[f32; 4]; 4]) -> Self {
        let mut values = [[0.0; 4]; 4];
        for row in 0..4 {
            for column in 0..4 {
                values[row][column] = matrix[column][row];
            }
        }
        Self { values }
    }

    fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        let rotation = rotation.normalized();
        let x2 = rotation.x + rotation.x;
        let y2 = rotation.y + rotation.y;
        let z2 = rotation.z + rotation.z;
        let xx = rotation.x * x2;
        let xy = rotation.x * y2;
        let xz = rotation.x * z2;
        let yy = rotation.y * y2;
        let yz = rotation.y * z2;
        let zz = rotation.z * z2;
        let wx = rotation.w * x2;
        let wy = rotation.w * y2;
        let wz = rotation.w * z2;

        Self {
            values: [
                [(1.0 - (yy + zz)) * scale.x, (xy - wz) * scale.y, (xz + wy) * scale.z, translation.x],
                [(xy + wz) * scale.x, (1.0 - (xx + zz)) * scale.y, (yz - wx) * scale.z, translation.y],
                [(xz - wy) * scale.x, (yz + wx) * scale.y, (1.0 - (xx + yy)) * scale.z, translation.z],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    fn mul(self, rhs: Self) -> Self {
        let mut values = [[0.0; 4]; 4];
        for row in 0..4 {
            for column in 0..4 {
                values[row][column] = self.values[row][0] * rhs.values[0][column]
                    + self.values[row][1] * rhs.values[1][column]
                    + self.values[row][2] * rhs.values[2][column]
                    + self.values[row][3] * rhs.values[3][column];
            }
        }
        Self { values }
    }

    fn transform_point(self, point: Vec3) -> Vec3 {
        Vec3::new(
            self.values[0][0] * point.x + self.values[0][1] * point.y + self.values[0][2] * point.z + self.values[0][3],
            self.values[1][0] * point.x + self.values[1][1] * point.y + self.values[1][2] * point.z + self.values[1][3],
            self.values[2][0] * point.x + self.values[2][1] * point.y + self.values[2][2] * point.z + self.values[2][3],
        )
    }
}

impl Quat {
    fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    fn identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    fn normalized(self) -> Self {
        let length = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        if length <= f32::EPSILON {
            return Self::identity();
        }
        Self::new(self.x / length, self.y / length, self.z / length, self.w / length)
    }

    fn slerp(self, mut rhs: Self, t: f32) -> Self {
        let mut cos_theta = self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w;
        if cos_theta < 0.0 {
            rhs = Self::new(-rhs.x, -rhs.y, -rhs.z, -rhs.w);
            cos_theta = -cos_theta;
        }

        if cos_theta > 0.9995 {
            return Self::new(
                self.x + (rhs.x - self.x) * t,
                self.y + (rhs.y - self.y) * t,
                self.z + (rhs.z - self.z) * t,
                self.w + (rhs.w - self.w) * t,
            )
            .normalized();
        }

        let theta = cos_theta.acos();
        let sin_theta = theta.sin();
        let left = ((1.0 - t) * theta).sin() / sin_theta;
        let right = (t * theta).sin() / sin_theta;
        Self::new(
            self.x * left + rhs.x * right,
            self.y * left + rhs.y * right,
            self.z * left + rhs.z * right,
            self.w * left + rhs.w * right,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct VisualTransform {
    center_x: f32,
    center_y: f32,
    center_z: f32,
    rotation_x: f64,
    rotation_y: f64,
    rotation_z: f64,
    scale_x: f32,
    scale_y: f32,
}

impl SceneRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build_vertices(&mut self, width: u32, height: u32, scene: &RenderScene<'_>) -> Result<&[GpuVertex]> {
        let mut vertices = mem::take(&mut self.vertices);
        vertices.clear();
        vertices.reserve(scene.objects.len() * 36 + 4096);
        for object in scene.objects {
            if object.visual_kind == ObjectVisualKind::FoxBuddy {
                let mesh = fox_buddy_animated_mesh(
                    object.body.width.min(object.body.height).max(1.0),
                    scene.elapsed_seconds,
                    object.body.velocity,
                );
                let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
                for triangle in &mesh.triangles {
                    emit_draw_triangle(
                        &mut vertices,
                        transform_triangle(*triangle, transform, scene.elapsed_seconds),
                    );
                }
                continue;
            }

            let mesh = self.mesh_for_object(object)?;
            let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
            for triangle in &mesh.triangles {
                emit_draw_triangle(
                    &mut vertices,
                    transform_triangle(*triangle, transform, scene.elapsed_seconds),
                );
            }
        }

        emit_cursor(&mut vertices, width, height, scene.cursor);
        emit_panels(&mut vertices, width, height, scene.hud);
        self.vertices = vertices;
        Ok(&self.vertices)
    }

    fn mesh_for_object(&mut self, object: &ObjectState) -> Result<&Mesh> {
        let size = object.body.width.min(object.body.height).max(1.0);
        if object.visual_kind == ObjectVisualKind::ImportedModel {
            return match object.model_source_path.as_deref() {
                Some(path) => self.imported_model_mesh(path, size, object.base_color),
                None => self.generated_mesh(MeshCacheKey::for_object(object), || cube_mesh(size, object.base_color)),
            };
        }

        let key = MeshCacheKey::for_object(object);
        match object.visual_kind {
            ObjectVisualKind::Cube => self.generated_mesh(key, || cube_mesh(size, object.base_color)),
            ObjectVisualKind::Dice => self.generated_mesh(key, || dice_mesh(size)),
            ObjectVisualKind::Crystal => self.generated_mesh(key, || crystal_mesh(size, object.base_color)),
            ObjectVisualKind::Satellite => self.generated_mesh(key, || satellite_mesh(size, object.base_color)),
            ObjectVisualKind::Ball => self.generated_mesh(key, || ball_mesh(size, object.base_color)),
            ObjectVisualKind::Pyramid => self.generated_mesh(key, || pyramid_mesh(size, object.base_color)),
            ObjectVisualKind::Barrel => self.generated_mesh(key, || barrel_mesh(size, object.base_color)),
            ObjectVisualKind::Ring => self.generated_mesh(key, || ring_mesh(size, object.base_color)),
            ObjectVisualKind::Star => self.generated_mesh(key, || star_mesh(size, object.base_color)),
            ObjectVisualKind::GamePlank => self.generated_mesh(key, || {
                rectangular_prism_mesh(object.body.width.max(1.0), object.body.height.max(1.0), size * 0.42, object.base_color)
            }),
            ObjectVisualKind::GameTarget => self.generated_mesh(key, || target_mesh(size, object.base_color)),
            ObjectVisualKind::FoxBuddy => self.generated_mesh(key, || fox_buddy_mesh(size)),
            ObjectVisualKind::RobotBuddy => self.generated_mesh(key, || robot_buddy_mesh(size, object.base_color)),
            ObjectVisualKind::DvdLogo => self.generated_mesh(key, || {
                dvd_logo_mesh(object.body.width.max(1.0), object.body.height.max(1.0), object.base_color)
            }),
            ObjectVisualKind::ImportedModel => unreachable!(),
        }
    }

    fn generated_mesh(&mut self, key: MeshCacheKey, create: impl FnOnce() -> Mesh) -> Result<&Mesh> {
        if !self.generated_models.contains_key(&key) {
            self.generated_models.insert(key, create());
        }

        Ok(self
            .generated_models
            .get(&key)
            .expect("generated mesh cache should contain inserted key"))
    }

    fn imported_model_mesh(&mut self, path: &str, target_size: f32, tint: AppColor) -> Result<&Mesh> {
        let cache_key = format!(
            "{path}|{target_size:.3}|{:02x}{:02x}{:02x}{:02x}",
            tint.a, tint.r, tint.g, tint.b
        );
        if !self.imported_models.contains_key(&cache_key) {
            let imported = load_mesh(path, target_size)?;
            self.imported_models.insert(cache_key.clone(), tint_mesh(imported, tint));
        }

        Ok(self
            .imported_models
            .get(&cache_key)
            .expect("imported mesh cache should contain inserted key"))
    }
}

impl MeshCacheKey {
    fn for_object(object: &ObjectState) -> Self {
        let size = object.body.width.min(object.body.height).max(1.0);
        Self {
            kind: match object.visual_kind {
                ObjectVisualKind::Cube => 0,
                ObjectVisualKind::Dice => 1,
                ObjectVisualKind::Crystal => 2,
                ObjectVisualKind::Satellite => 3,
                ObjectVisualKind::DvdLogo => 4,
                ObjectVisualKind::Ball => 5,
                ObjectVisualKind::Pyramid => 6,
                ObjectVisualKind::Barrel => 7,
                ObjectVisualKind::Ring => 8,
                ObjectVisualKind::Star => 9,
                ObjectVisualKind::GamePlank => 10,
                ObjectVisualKind::GameTarget => 11,
                ObjectVisualKind::FoxBuddy => 12,
                ObjectVisualKind::RobotBuddy => 13,
                ObjectVisualKind::ImportedModel => 14,
            },
            width_milli: quantize_size(object.body.width.max(1.0)),
            height_milli: quantize_size(object.body.height.max(1.0)),
            size_milli: quantize_size(size),
            color: object.base_color,
        }
    }
}

fn quantize_size(value: f32) -> i32 {
    (value * 1000.0).round() as i32
}

const SCREEN_OVERLAY_Z: f32 = 900.0;

fn screen_overlay_z(layer: f32) -> f32 {
    SCREEN_OVERLAY_Z + layer.clamp(-16.0, 16.0)
}

fn emit_panels(vertices: &mut Vec<GpuVertex>, width: u32, height: u32, hud: &HudState) {
    let mut top = 12i32;

    let _ = &hud.status_message;

    for panel in &hud.panels {
        emit_panel(vertices, width, height, 12, top, &panel.title, &panel.lines, &panel.footer);
        top += ((panel.lines.len() + panel.footer.len() + 2) as i32 * 12).max(72) + 10;
    }
}

fn emit_panel(
    vertices: &mut Vec<GpuVertex>,
    width: u32,
    height: u32,
    left: i32,
    top: i32,
    title: &str,
    lines: &[PanelLine],
    footer: &[String],
) {
    let content_line_count = 1 + lines.len() + footer.len();
    let panel_width = 360;
    let panel_height = 18 + (content_line_count as i32 * 12) + 12;
    emit_rect(
        vertices,
        width,
        height,
        left,
        top,
        panel_width,
        panel_height,
        AppColor::from_argb(180, 8, 16, 30),
        0.05,
    );
    emit_rect_outline(
        vertices,
        width,
        height,
        left,
        top,
        panel_width,
        panel_height,
        AppColor::from_argb(220, 160, 220, 255),
        0.04,
    );

    emit_text(
        vertices,
        width,
        height,
        left + 10,
        top + 8,
        title,
        AppColor::from_rgb(185, 245, 255),
        2,
        0.03,
    );

    let mut y = top + 24;
    for line in lines {
        if line.selected {
            emit_rect(
                vertices,
                width,
                height,
                left + 8,
                y - 2,
                panel_width - 16,
                11,
                AppColor::from_argb(120, 36, 74, 140),
                0.035,
            );
        }

        emit_text(
            vertices,
            width,
            height,
            left + 12,
            y,
            &line.text,
            if line.selected {
                AppColor::from_rgb(255, 240, 170)
            } else {
                AppColor::from_rgb(210, 224, 240)
            },
            1,
            0.03,
        );
        y += 12;
    }

    for line in footer {
        emit_text(
            vertices,
            width,
            height,
            left + 12,
            y,
            line,
            AppColor::from_rgb(154, 184, 208),
            1,
            0.03,
        );
        y += 12;
    }
}

fn emit_cursor(vertices: &mut Vec<GpuVertex>, width: u32, height: u32, cursor: Vector2) {
    let x = cursor.x.round() as i32;
    let y = cursor.y.round() as i32;
    emit_rect(
        vertices,
        width,
        height,
        x - 1,
        y - 10,
        3,
        21,
        AppColor::from_argb(220, 255, 255, 255),
        0.01,
    );
    emit_rect(
        vertices,
        width,
        height,
        x - 10,
        y - 1,
        21,
        3,
        AppColor::from_argb(220, 255, 255, 255),
        0.01,
    );
}

fn emit_draw_triangle(vertices: &mut Vec<GpuVertex>, triangle: DrawTriangle) {
    let color = color_to_f32(triangle.color, triangle.alpha);
    for point in triangle.points {
        vertices.push(GpuVertex {
            position: [point.x, point.y, point.z],
            color,
        });
    }
}

fn emit_rect(
    vertices: &mut Vec<GpuVertex>,
    _width: u32,
    _height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: AppColor,
    depth: f32,
) {
    let left = x as f32;
    let top = y as f32;
    let right = (x + w) as f32;
    let bottom = (y + h) as f32;
    let z = screen_overlay_z(depth);
    let p0 = Vec3::new(left, top, z);
    let p1 = Vec3::new(right, top, z);
    let p2 = Vec3::new(right, bottom, z);
    let p3 = Vec3::new(left, bottom, z);
    let alpha = color.a;
    let draw = DrawTriangle {
        points: [p0, p1, p2],
        color,
        alpha,
    };
    emit_draw_triangle(vertices, draw);
    emit_draw_triangle(
        vertices,
        DrawTriangle {
            points: [p0, p2, p3],
            color,
            alpha,
        },
    );
}

fn emit_rect_outline(
    vertices: &mut Vec<GpuVertex>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: AppColor,
    depth: f32,
) {
    emit_rect(vertices, width, height, x, y, w, 1, color, depth);
    emit_rect(vertices, width, height, x, y + h - 1, w, 1, color, depth);
    emit_rect(vertices, width, height, x, y, 1, h, color, depth);
    emit_rect(vertices, width, height, x + w - 1, y, 1, h, color, depth);
}

fn emit_text(
    vertices: &mut Vec<GpuVertex>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    text: &str,
    color: AppColor,
    scale: i32,
    depth: f32,
) {
    let mut cursor_x = x;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }

        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row_idx, row) in glyph.iter().enumerate() {
                for col_idx in 0..8usize {
                    if row & (1 << col_idx) == 0 {
                        continue;
                    }

                    emit_rect(
                        vertices,
                        width,
                        height,
                        cursor_x + col_idx as i32 * scale,
                        y + row_idx as i32 * scale,
                        scale,
                        scale,
                        color,
                        depth,
                    );
                }
            }
        }

        cursor_x += 8 * scale;
    }
}

fn color_to_f32(color: AppColor, alpha_override: u8) -> [f32; 4] {
    [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        alpha_override as f32 / 255.0,
    ]
}

fn compute_visual_transform(object: &ObjectState, bounds: RectF, elapsed_seconds: f64) -> VisualTransform {
    let center_x = object.body.position.x + (object.body.width * 0.5);
    let center_y = object.body.position.y + (object.body.height * 0.5);
    let mut center_z = 0.0f32;
    let rotation_x = object.rotation_x;
    let mut rotation_y = object.rotation_y;
    let mut rotation_z = object.rotation_z;
    let mut scale_x = compute_stretch_x(object);
    let mut scale_y = compute_impact_scale(object, bounds);
    let phase = get_phase(object.id);

    match object.visual_kind {
        ObjectVisualKind::Crystal => {
            let shimmer = ((elapsed_seconds * 3.8) + phase).sin();
            center_z += 22.0 + (shimmer as f32 * 12.0);
            scale_x *= 1.0 + (shimmer as f32 * 0.035);
            scale_y *= 1.0 + (((elapsed_seconds * 3.1) + phase).cos() as f32 * 0.055);
            rotation_y += ((elapsed_seconds * 1.1) + phase).sin() * 7.0;
        },
        ObjectVisualKind::Satellite => {
            let wobble = ((elapsed_seconds * 2.2) + phase).sin();
            center_z += 30.0 + (wobble as f32 * 7.0);
            rotation_z += wobble * 11.0;
            rotation_y += ((elapsed_seconds * 1.4) + phase).cos() * 9.0;
        },
        ObjectVisualKind::Dice => {
            center_z += 8.0;
        },
        ObjectVisualKind::Ball => {
            center_z += 10.0;
        },
        ObjectVisualKind::Pyramid => {
            center_z += 6.0;
        },
        ObjectVisualKind::Barrel => {
            center_z += 8.0;
        },
        ObjectVisualKind::Ring => {
            center_z += 14.0;
            rotation_y += ((elapsed_seconds * 0.8) + phase).sin() * 6.0;
        },
        ObjectVisualKind::Star => {
            center_z += 12.0;
            rotation_z += ((elapsed_seconds * 1.2) + phase).sin() * 5.0;
        },
        ObjectVisualKind::GamePlank => {
            center_z += 4.0;
        },
        ObjectVisualKind::GameTarget => {
            center_z += 12.0;
        },
        ObjectVisualKind::FoxBuddy => {
            center_z += 18.0;
            let facing_velocity = if object.body.motor_enabled {
                object.body.motor_velocity_x
            } else {
                object.body.velocity.x
            };
            if facing_velocity > 1.0 {
                scale_x *= -1.0;
            }
        },
        ObjectVisualKind::RobotBuddy => {
            center_z += 14.0;
            let facing_velocity = if object.body.motor_enabled {
                object.body.motor_velocity_x
            } else {
                object.body.velocity.x
            };
            if facing_velocity < -1.0 {
                scale_x *= -1.0;
            }
            rotation_z += (object.body.velocity.x as f64 * 0.01).clamp(-4.0, 4.0);
        },
        ObjectVisualKind::ImportedModel => {
            center_z += 12.0;
        },
        ObjectVisualKind::DvdLogo => {},
        ObjectVisualKind::Cube => {},
    }

    VisualTransform {
        center_x,
        center_y,
        center_z,
        rotation_x,
        rotation_y,
        rotation_z,
        scale_x,
        scale_y,
    }
}

fn compute_impact_scale(object: &ObjectState, bounds: RectF) -> f32 {
    if object.body.position.y + object.body.height >= bounds.bottom() - 1.0 && object.body.velocity.y.abs() > 300.0 {
        0.96
    } else {
        1.0
    }
}

fn compute_stretch_x(object: &ObjectState) -> f32 {
    1.0 + ((object.body.velocity.x.abs() / 2200.0).min(1.0) * 0.03)
}

fn get_phase(id: u64) -> f64 {
    ((id as u32 as f64) / u32::MAX as f64) * std::f64::consts::TAU
}

fn transform_triangle(source: SourceTriangle, transform: VisualTransform, elapsed_seconds: f64) -> DrawTriangle {
    let _ = elapsed_seconds;
    let points = source.vertices.map(|vertex| project_vertex(vertex, transform));
    let color = apply_natural_light(source.color, &points);

    DrawTriangle {
        points,
        color,
        alpha: source.alpha,
    }
}

fn project_vertex(vertex: Vec3, transform: VisualTransform) -> Vec3 {
    let mut point = Vec3::new(vertex.x * transform.scale_x, vertex.y * transform.scale_y, vertex.z);
    point = rotate_x(point, transform.rotation_x as f32);
    point = rotate_y(point, transform.rotation_y as f32);
    point = rotate_z(point, transform.rotation_z as f32);
    point.x += transform.center_x;
    point.y += transform.center_y;
    point.z += transform.center_z;
    point
}

fn rotate_x(point: Vec3, angle_degrees: f32) -> Vec3 {
    let radians = angle_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    Vec3::new(point.x, point.y * cos - point.z * sin, point.y * sin + point.z * cos)
}

fn rotate_y(point: Vec3, angle_degrees: f32) -> Vec3 {
    let radians = angle_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    Vec3::new(point.x * cos + point.z * sin, point.y, -point.x * sin + point.z * cos)
}

fn rotate_z(point: Vec3, angle_degrees: f32) -> Vec3 {
    let radians = angle_degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    Vec3::new(point.x * cos - point.y * sin, point.x * sin + point.y * cos, point.z)
}

fn apply_natural_light(color: AppColor, points: &[Vec3; 3]) -> AppColor {
    let edge_a = Vec3::new(
        points[1].x - points[0].x,
        points[1].y - points[0].y,
        points[1].z - points[0].z,
    );
    let edge_b = Vec3::new(
        points[2].x - points[0].x,
        points[2].y - points[0].y,
        points[2].z - points[0].z,
    );
    let normal = edge_a.cross(edge_b).normalized();
    let light_dir = Vec3::new(-0.36, -0.58, 0.73).normalized();
    let fill_dir = Vec3::new(0.55, 0.28, 0.30).normalized();
    let key = normal.dot(light_dir).max(0.0);
    let fill = normal.dot(fill_dir).max(0.0);
    let rim = (1.0 - normal.z.abs()).max(0.0).powf(1.6);
    let shade = 0.46 + (key * 0.56) + (fill * 0.16) + (rim * 0.10);
    let lit = scale_color(color, shade.clamp(0.34, 1.22));
    let warmth = (key * 9.0).round() as i16;
    AppColor::from_argb(
        lit.a,
        add_channel(lit.r, warmth),
        add_channel(lit.g, (warmth as f32 * 0.55).round() as i16),
        add_channel(lit.b, -(warmth / 3)),
    )
}

fn add_channel(value: u8, delta: i16) -> u8 {
    (value as i16 + delta).clamp(0, 255) as u8
}

fn scale_channel(value: u8, factor: f32) -> u8 {
    ((value as f32 * factor).round()).clamp(0.0, 255.0) as u8
}

fn cube_mesh(size: f32, base_color: AppColor) -> Mesh {
    rectangular_prism_mesh(size, size, size, base_color)
}

fn rectangular_prism_mesh(width: f32, height: f32, depth: f32, base_color: AppColor) -> Mesh {
    let hx = width * 0.5;
    let hy = height * 0.5;
    let hz = depth.max(2.0) * 0.5;
    let faces = [
        (quad(Vec3::new(-hx, -hy, hz), Vec3::new(hx, -hy, hz), Vec3::new(hx, hy, hz), Vec3::new(-hx, hy, hz)), scale_color(base_color, 1.10)),
        (quad(Vec3::new(-hx, -hy, -hz), Vec3::new(-hx, hy, -hz), Vec3::new(hx, hy, -hz), Vec3::new(hx, -hy, -hz)), scale_color(base_color, 0.62)),
        (quad(Vec3::new(-hx, -hy, -hz), Vec3::new(-hx, -hy, hz), Vec3::new(-hx, hy, hz), Vec3::new(-hx, hy, -hz)), scale_color(base_color, 0.78)),
        (quad(Vec3::new(hx, -hy, -hz), Vec3::new(hx, hy, -hz), Vec3::new(hx, hy, hz), Vec3::new(hx, -hy, hz)), scale_color(base_color, 0.56)),
        (quad(Vec3::new(-hx, -hy, -hz), Vec3::new(hx, -hy, -hz), Vec3::new(hx, -hy, hz), Vec3::new(-hx, -hy, hz)), scale_color(base_color, 0.96)),
        (quad(Vec3::new(-hx, hy, -hz), Vec3::new(-hx, hy, hz), Vec3::new(hx, hy, hz), Vec3::new(hx, hy, -hz)), scale_color(base_color, 0.70)),
    ];

    let mut triangles = Vec::new();
    for (face, color) in faces {
        triangles.extend(face.into_iter().map(|vertices| SourceTriangle {
            vertices,
            color,
            alpha: 255,
        }));
    }

    Mesh { triangles }
}

fn dice_mesh(size: f32) -> Mesh {
    let mut mesh = cube_mesh(size, AppColor::from_rgb(245, 245, 240));
    let hs = size * 0.5;
    let pip_radius = size * 0.07;
    let pip_surface_offset = size * 0.006;
    let pip_offset = size * 0.22;
    let pip_color = AppColor::from_rgb(42, 48, 58);

    add_z_face_pips(
        &mut mesh.triangles,
        hs + pip_surface_offset,
        pip_radius,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, pip_offset)],
    );
    add_z_face_pips(
        &mut mesh.triangles,
        -hs - pip_surface_offset,
        pip_radius,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (0.0, 0.0), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );
    add_x_face_pips(
        &mut mesh.triangles,
        hs + pip_surface_offset,
        pip_radius,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, 0.0), (-pip_offset, pip_offset)],
    );
    add_x_face_pips(
        &mut mesh.triangles,
        -hs - pip_surface_offset,
        pip_radius,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );
    add_y_face_pips(
        &mut mesh.triangles,
        -hs - pip_surface_offset,
        pip_radius,
        pip_color,
        &[(0.0, 0.0)],
    );
    add_y_face_pips(
        &mut mesh.triangles,
        hs + pip_surface_offset,
        pip_radius,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (-pip_offset, 0.0), (pip_offset, 0.0), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );

    mesh
}

fn add_z_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_z: f32,
    pip_radius: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_x, center_y) in positions {
        add_disk(
            triangles,
            Vec3::new(center_x, center_y, pip_z),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            pip_radius,
            color,
            pip_z >= 0.0,
        );
    }
}

fn add_x_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_x: f32,
    pip_radius: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_z, center_y) in positions {
        add_disk(
            triangles,
            Vec3::new(pip_x, center_y, center_z),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 1.0, 0.0),
            pip_radius,
            color,
            pip_x < 0.0,
        );
    }
}

fn add_y_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_y: f32,
    pip_radius: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_x, center_z) in positions {
        add_disk(
            triangles,
            Vec3::new(center_x, pip_y, center_z),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            pip_radius,
            color,
            pip_y < 0.0,
        );
    }
}

fn add_disk(
    triangles: &mut Vec<SourceTriangle>,
    center: Vec3,
    axis_u: Vec3,
    axis_v: Vec3,
    radius: f32,
    color: AppColor,
    outward_winding: bool,
) {
    const SEGMENTS: usize = 18;
    for index in 0..SEGMENTS {
        let a0 = (index as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let a1 = ((index + 1) as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let p0 = offset_disk_point(center, axis_u, axis_v, radius, a0);
        let p1 = offset_disk_point(center, axis_u, axis_v, radius, a1);
        let vertices = if outward_winding { [center, p0, p1] } else { [center, p1, p0] };
        triangles.push(SourceTriangle {
            vertices,
            color,
            alpha: 255,
        });
    }
}

fn offset_disk_point(center: Vec3, axis_u: Vec3, axis_v: Vec3, radius: f32, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    Vec3::new(
        center.x + ((axis_u.x * cos + axis_v.x * sin) * radius),
        center.y + ((axis_u.y * cos + axis_v.y * sin) * radius),
        center.z + ((axis_u.z * cos + axis_v.z * sin) * radius),
    )
}

fn crystal_mesh(size: f32, base_color: AppColor) -> Mesh {
    let radius = size * 0.34;
    let top = Vec3::new(0.0, -size * 0.5, 0.0);
    let bottom = Vec3::new(0.0, size * 0.5, 0.0);
    let front = Vec3::new(0.0, 0.0, radius);
    let right = Vec3::new(radius, 0.0, 0.0);
    let back = Vec3::new(0.0, 0.0, -radius);
    let left = Vec3::new(-radius, 0.0, 0.0);

    let mut triangles = Vec::new();
    let outer = [
        ([top, front, right], scale_color(base_color, 1.12), 255),
        ([top, right, back], scale_color(base_color, 0.78), 255),
        ([top, back, left], scale_color(base_color, 0.56), 255),
        ([top, left, front], scale_color(base_color, 0.78), 255),
        ([bottom, right, front], scale_color(base_color, 1.12), 255),
        ([bottom, back, right], scale_color(base_color, 0.78), 255),
        ([bottom, left, back], scale_color(base_color, 0.56), 255),
        ([bottom, front, left], scale_color(base_color, 0.78), 255),
    ];

    for (vertices, color, alpha) in outer {
        triangles.push(SourceTriangle {
            vertices,
            color,
            alpha,
        });
    }

    Mesh { triangles }
}

fn ball_mesh(size: f32, base_color: AppColor) -> Mesh {
    const LATITUDES: usize = 7;
    const LONGITUDES: usize = 14;
    let radius = size * 0.5;
    let mut triangles = Vec::new();

    for lat in 0..LATITUDES {
        let theta0 = std::f32::consts::PI * (lat as f32 / LATITUDES as f32);
        let theta1 = std::f32::consts::PI * ((lat + 1) as f32 / LATITUDES as f32);
        for lon in 0..LONGITUDES {
            let phi0 = std::f32::consts::TAU * (lon as f32 / LONGITUDES as f32);
            let phi1 = std::f32::consts::TAU * ((lon + 1) as f32 / LONGITUDES as f32);
            let p00 = sphere_point(radius, theta0, phi0);
            let p01 = sphere_point(radius, theta0, phi1);
            let p10 = sphere_point(radius, theta1, phi0);
            let p11 = sphere_point(radius, theta1, phi1);
            let shade = 0.64 + (lat as f32 / LATITUDES as f32 * 0.34);
            let color = scale_color(base_color, shade);
            if lat == 0 {
                triangles.push(SourceTriangle {
                    vertices: [p00, p10, p11],
                    color,
                    alpha: 255,
                });
            } else if lat + 1 == LATITUDES {
                triangles.push(SourceTriangle {
                    vertices: [p00, p10, p01],
                    color,
                    alpha: 255,
                });
            } else {
                triangles.push(SourceTriangle {
                    vertices: [p00, p10, p11],
                    color,
                    alpha: 255,
                });
                triangles.push(SourceTriangle {
                    vertices: [p00, p11, p01],
                    color,
                    alpha: 255,
                });
            }
        }
    }

    Mesh { triangles }
}

fn target_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut mesh = ball_mesh(size, base_color);
    let face_z = size * 0.44;
    let eye_color = AppColor::from_rgb(32, 62, 40);
    let cheek_color = scale_color(base_color, 1.32);
    add_disk(
        &mut mesh.triangles,
        Vec3::new(-size * 0.14, -size * 0.08, face_z),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.045,
        eye_color,
        true,
    );
    add_disk(
        &mut mesh.triangles,
        Vec3::new(size * 0.14, -size * 0.08, face_z),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.045,
        eye_color,
        true,
    );
    add_disk(
        &mut mesh.triangles,
        Vec3::new(0.0, size * 0.13, face_z + size * 0.01),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.075,
        cheek_color,
        true,
    );
    mesh
}

fn robot_buddy_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut triangles = Vec::new();
    let body_color = scale_color(base_color, 0.94);
    let head_color = scale_color(base_color, 1.16);
    let trim_color = AppColor::from_rgb(42, 52, 60);
    let glow_color = AppColor::from_rgb(110, 238, 255);
    let claw_color = AppColor::from_rgb(255, 226, 92);

    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.78, size * 0.62, size * 0.42, body_color),
        Vec3::new(0.0, size * 0.12, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.46, size * 0.08, size * 0.3, trim_color),
        Vec3::new(0.02 * size, -size * 0.22, -size * 0.02),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.6, size * 0.42, size * 0.38, head_color),
        Vec3::new(size * 0.02, -size * 0.35, size * 0.04),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.36, size * 0.1, size * 0.06, AppColor::from_rgb(30, 48, 64)),
        Vec3::new(size * 0.19, -size * 0.36, size * 0.25),
    );
    append_mesh_offset(
        &mut triangles,
        ball_mesh(size * 0.055, glow_color),
        Vec3::new(size * 0.48, -size * 0.36, size * 0.27),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.12, size * 0.25, size * 0.16, scale_color(base_color, 0.72)),
        Vec3::new(-size * 0.5, size * 0.1, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.4, size * 0.12, size * 0.16, scale_color(base_color, 0.82)),
        Vec3::new(size * 0.62, size * 0.02, size * 0.02),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.08, size * 0.34, size * 0.08, claw_color),
        Vec3::new(size * 0.86, size * 0.02, size * 0.05),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.16, size * 0.06, size * 0.08, claw_color),
        Vec3::new(size * 0.94, -size * 0.13, size * 0.06),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.16, size * 0.06, size * 0.08, claw_color),
        Vec3::new(size * 0.94, size * 0.17, size * 0.06),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.3, size * 0.14, size * 0.04, AppColor::from_rgb(66, 84, 92)),
        Vec3::new(size * 0.04, size * 0.14, size * 0.24),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.9, size * 0.18, size * 0.26, trim_color),
        Vec3::new(0.0, size * 0.54, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        ball_mesh(size * 0.18, AppColor::from_rgb(44, 48, 54)),
        Vec3::new(-size * 0.33, size * 0.6, size * 0.02),
    );
    append_mesh_offset(
        &mut triangles,
        ball_mesh(size * 0.18, AppColor::from_rgb(44, 48, 54)),
        Vec3::new(size * 0.33, size * 0.6, size * 0.02),
    );
    Mesh { triangles }
}

fn append_mesh_offset(triangles: &mut Vec<SourceTriangle>, mesh: Mesh, offset: Vec3) {
    triangles.extend(mesh.triangles.into_iter().map(|mut triangle| {
        for vertex in &mut triangle.vertices {
            vertex.x += offset.x;
            vertex.y += offset.y;
            vertex.z += offset.z;
        }
        triangle
    }));
}

fn sphere_point(radius: f32, theta: f32, phi: f32) -> Vec3 {
    let sin_theta = theta.sin();
    Vec3::new(
        radius * sin_theta * phi.cos(),
        radius * theta.cos(),
        radius * sin_theta * phi.sin(),
    )
}

fn pyramid_mesh(size: f32, base_color: AppColor) -> Mesh {
    let hs = size * 0.5;
    let top = Vec3::new(0.0, -hs, 0.0);
    let base_y = hs * 0.72;
    let p0 = Vec3::new(-hs, base_y, -hs);
    let p1 = Vec3::new(hs, base_y, -hs);
    let p2 = Vec3::new(hs, base_y, hs);
    let p3 = Vec3::new(-hs, base_y, hs);
    Mesh {
        triangles: vec![
            SourceTriangle { vertices: [top, p0, p1], color: scale_color(base_color, 1.14), alpha: 255 },
            SourceTriangle { vertices: [top, p1, p2], color: scale_color(base_color, 0.88), alpha: 255 },
            SourceTriangle { vertices: [top, p2, p3], color: scale_color(base_color, 0.68), alpha: 255 },
            SourceTriangle { vertices: [top, p3, p0], color: scale_color(base_color, 0.98), alpha: 255 },
            SourceTriangle { vertices: [p0, p2, p1], color: scale_color(base_color, 0.52), alpha: 255 },
            SourceTriangle { vertices: [p0, p3, p2], color: scale_color(base_color, 0.52), alpha: 255 },
        ],
    }
}

fn barrel_mesh(size: f32, base_color: AppColor) -> Mesh {
    const SEGMENTS: usize = 18;
    let radius = size * 0.43;
    let half_height = size * 0.46;
    let mut triangles = Vec::new();
    let top = Vec3::new(0.0, -half_height, 0.0);
    let bottom = Vec3::new(0.0, half_height, 0.0);

    for i in 0..SEGMENTS {
        let a0 = std::f32::consts::TAU * (i as f32 / SEGMENTS as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / SEGMENTS as f32);
        let p0 = Vec3::new(radius * a0.cos(), -half_height, radius * a0.sin());
        let p1 = Vec3::new(radius * a1.cos(), -half_height, radius * a1.sin());
        let p2 = Vec3::new(radius * a1.cos(), half_height, radius * a1.sin());
        let p3 = Vec3::new(radius * a0.cos(), half_height, radius * a0.sin());
        let shade = if i % 2 == 0 { 1.05 } else { 0.82 };
        let side_color = scale_color(base_color, shade);
        triangles.push(SourceTriangle { vertices: [p0, p1, p2], color: side_color, alpha: 255 });
        triangles.push(SourceTriangle { vertices: [p0, p2, p3], color: side_color, alpha: 255 });
        triangles.push(SourceTriangle { vertices: [top, p1, p0], color: scale_color(base_color, 1.18), alpha: 255 });
        triangles.push(SourceTriangle { vertices: [bottom, p3, p2], color: scale_color(base_color, 0.58), alpha: 255 });
    }

    Mesh { triangles }
}

fn ring_mesh(size: f32, base_color: AppColor) -> Mesh {
    const MAJOR_SEGMENTS: usize = 20;
    const MINOR_SEGMENTS: usize = 8;
    let major = size * 0.34;
    let minor = size * 0.12;
    let mut triangles = Vec::new();

    for i in 0..MAJOR_SEGMENTS {
        let a0 = std::f32::consts::TAU * (i as f32 / MAJOR_SEGMENTS as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / MAJOR_SEGMENTS as f32);
        for j in 0..MINOR_SEGMENTS {
            let b0 = std::f32::consts::TAU * (j as f32 / MINOR_SEGMENTS as f32);
            let b1 = std::f32::consts::TAU * ((j + 1) as f32 / MINOR_SEGMENTS as f32);
            let p00 = torus_point(major, minor, a0, b0);
            let p01 = torus_point(major, minor, a0, b1);
            let p10 = torus_point(major, minor, a1, b0);
            let p11 = torus_point(major, minor, a1, b1);
            let shade = 0.72 + (b0.cos().max(0.0) * 0.36);
            let color = scale_color(base_color, shade);
            triangles.push(SourceTriangle { vertices: [p00, p10, p11], color, alpha: 255 });
            triangles.push(SourceTriangle { vertices: [p00, p11, p01], color, alpha: 255 });
        }
    }

    Mesh { triangles }
}

fn torus_point(major: f32, minor: f32, a: f32, b: f32) -> Vec3 {
    let radial = major + minor * b.cos();
    Vec3::new(radial * a.cos(), minor * b.sin(), radial * a.sin())
}

fn star_mesh(size: f32, base_color: AppColor) -> Mesh {
    const POINTS: usize = 10;
    let outer = size * 0.50;
    let inner = size * 0.23;
    let depth = size * 0.12;
    let mut front = [Vec3::default(); POINTS];
    let mut back = [Vec3::default(); POINTS];
    for i in 0..POINTS {
        let radius = if i % 2 == 0 { outer } else { inner };
        let angle = -std::f32::consts::FRAC_PI_2 + (i as f32 / POINTS as f32) * std::f32::consts::TAU;
        front[i] = Vec3::new(radius * angle.cos(), radius * angle.sin(), depth);
        back[i] = Vec3::new(radius * angle.cos(), radius * angle.sin(), -depth);
    }

    let mut triangles = Vec::new();
    let center_front = Vec3::new(0.0, 0.0, depth);
    let center_back = Vec3::new(0.0, 0.0, -depth);
    for i in 0..POINTS {
        let next = (i + 1) % POINTS;
        triangles.push(SourceTriangle { vertices: [center_front, front[i], front[next]], color: scale_color(base_color, 1.14), alpha: 255 });
        triangles.push(SourceTriangle { vertices: [center_back, back[next], back[i]], color: scale_color(base_color, 0.58), alpha: 255 });
        let side_color = scale_color(base_color, if i % 2 == 0 { 0.92 } else { 0.74 });
        triangles.push(SourceTriangle { vertices: [front[i], back[i], back[next]], color: side_color, alpha: 255 });
        triangles.push(SourceTriangle { vertices: [front[i], back[next], front[next]], color: side_color, alpha: 255 });
    }

    Mesh { triangles }
}

fn satellite_mesh(size: f32, accent_color: AppColor) -> Mesh {
    let mut triangles = Vec::new();
    let cuboids = [
        cuboid(-size * 0.18, -size * 0.18, -size * 0.18, size * 0.18, size * 0.18, size * 0.18, AppColor::from_rgb(168, 140, 74)),
        cuboid(-size * 0.62, -size * 0.12, -size * 0.04, -size * 0.24, size * 0.12, size * 0.04, scale_color(accent_color, 0.82)),
        cuboid(size * 0.24, -size * 0.12, -size * 0.04, size * 0.62, size * 0.12, size * 0.04, scale_color(accent_color, 0.82)),
        cuboid(-size * 0.03, -size * 0.42, -size * 0.03, size * 0.03, -size * 0.18, size * 0.03, AppColor::from_rgb(146, 154, 165)),
        cuboid(-size * 0.18, -size * 0.46, -size * 0.18, size * 0.18, -size * 0.40, size * 0.18, AppColor::from_rgb(210, 215, 223)),
        cuboid(-size * 0.08, size * 0.21, -size * 0.08, size * 0.08, size * 0.38, size * 0.08, AppColor::from_rgb(66, 76, 92)),
        cuboid(-size * 0.12, size * 0.34, -size * 0.12, size * 0.12, size * 0.52, size * 0.12, AppColor::from_rgb(210, 72, 72)),
    ];

    for cuboid in cuboids {
        triangles.extend(cuboid.triangles);
    }

    Mesh { triangles }
}

fn dvd_logo_mesh(width: f32, height: f32, accent_color: AppColor) -> Mesh {
    let mut mesh = cuboid(
        -width * 0.5,
        -height * 0.5,
        -height * 0.06,
        width * 0.5,
        height * 0.5,
        height * 0.06,
        scale_color(accent_color, 0.30),
    );
    let front_z = height * 0.06 + 0.08;

    add_front_rect(
        &mut mesh.triangles,
        -width * 0.42,
        -height * 0.42,
        width * 0.84,
        height * 0.26,
        front_z,
        AppColor::from_argb(164, 255, 255, 255),
    );
    add_front_text(
        &mut mesh.triangles,
        "DVD",
        height,
        front_z + 0.04,
        AppColor::from_rgb(255, 255, 255),
        0.088,
        -0.03,
    );
    add_front_text(
        &mut mesh.triangles,
        "VIDEO",
        height,
        front_z + 0.04,
        AppColor::from_rgb(255, 255, 255),
        0.040,
        0.19,
    );

    mesh
}

fn add_front_text(
    triangles: &mut Vec<SourceTriangle>,
    text: &str,
    height: f32,
    front_z: f32,
    color: AppColor,
    pixel_scale_by_height: f32,
    center_y_by_height: f32,
) {
    let pixel = (height * pixel_scale_by_height).max(1.0);
    let text_width = text.chars().count() as f32 * 8.0 * pixel;
    let start_x = -text_width * 0.5;
    let start_y = (height * center_y_by_height) - (4.0 * pixel);
    let mut cursor_x = start_x;
    for ch in text.chars() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row_idx, row) in glyph.iter().enumerate() {
                for col_idx in 0..8usize {
                    if row & (1 << col_idx) == 0 {
                        continue;
                    }

                    add_front_rect(
                        triangles,
                        cursor_x + col_idx as f32 * pixel,
                        start_y + row_idx as f32 * pixel,
                        pixel,
                        pixel,
                        front_z,
                        color,
                    );
                }
            }
        }
        cursor_x += 8.0 * pixel;
    }
}

fn add_front_rect(
    triangles: &mut Vec<SourceTriangle>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    z: f32,
    color: AppColor,
) {
    let p0 = Vec3::new(x, y, z);
    let p1 = Vec3::new(x + w, y, z);
    let p2 = Vec3::new(x + w, y + h, z);
    let p3 = Vec3::new(x, y + h, z);
    triangles.push(SourceTriangle {
        vertices: [p0, p1, p2],
        color,
        alpha: color.a,
    });
    triangles.push(SourceTriangle {
        vertices: [p0, p2, p3],
        color,
        alpha: color.a,
    });
}

fn cuboid(min_x: f32, min_y: f32, min_z: f32, max_x: f32, max_y: f32, max_z: f32, color: AppColor) -> Mesh {
    let p000 = Vec3::new(min_x, min_y, min_z);
    let p001 = Vec3::new(min_x, min_y, max_z);
    let p010 = Vec3::new(min_x, max_y, min_z);
    let p011 = Vec3::new(min_x, max_y, max_z);
    let p100 = Vec3::new(max_x, min_y, min_z);
    let p101 = Vec3::new(max_x, min_y, max_z);
    let p110 = Vec3::new(max_x, max_y, min_z);
    let p111 = Vec3::new(max_x, max_y, max_z);

    let mut triangles = Vec::new();
    for face in [
        quad(p001, p101, p111, p011),
        quad(p100, p000, p010, p110),
        quad(p000, p001, p011, p010),
        quad(p101, p100, p110, p111),
        quad(p000, p100, p101, p001),
        quad(p011, p111, p110, p010),
    ] {
        triangles.extend(face.into_iter().map(|vertices| SourceTriangle {
            vertices,
            color,
            alpha: 255,
        }));
    }

    Mesh { triangles }
}

fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> [[Vec3; 3]; 2] {
    [[a, b, c], [a, c, d]]
}

fn scale_color(color: AppColor, factor: f32) -> AppColor {
    AppColor::from_argb(
        color.a,
        scale_channel(color.r, factor),
        scale_channel(color.g, factor),
        scale_channel(color.b, factor),
    )
}

fn tint_mesh(mut mesh: Mesh, tint: AppColor) -> Mesh {
    for triangle in &mut mesh.triangles {
        triangle.color = AppColor::from_argb(
            triangle.color.a,
            multiply_channel(triangle.color.r, tint.r),
            multiply_channel(triangle.color.g, tint.g),
            multiply_channel(triangle.color.b, tint.b),
        );
    }
    mesh
}

fn multiply_channel(left: u8, right: u8) -> u8 {
    ((left as u16 * right as u16) / 255) as u8
}

fn load_mesh(path: &str, target_size: f32) -> Result<Mesh> {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut mesh = match extension.as_str() {
        "obj" => load_obj_mesh(path)?,
        "stl" => load_stl_mesh(path)?,
        "fbx" => bail!("FBX import is not implemented in the native port yet."),
        _ => bail!("Unsupported model format: {extension}"),
    };

    limit_imported_mesh(&mut mesh);
    normalize_mesh(&mut mesh, target_size)?;
    Ok(mesh)
}

fn load_obj_mesh(path: &str) -> Result<Mesh> {
    let (models, _) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            triangulate: true,
            single_index: true,
            ..Default::default()
        },
    )
    .with_context(|| format!("Failed to load OBJ file: {path}"))?;

    let mut triangles = Vec::new();
    for model in models {
        let mesh = model.mesh;
        for face in mesh.indices.chunks_exact(3) {
            let a = index_vertex(&mesh.positions, face[0] as usize);
            let b = index_vertex(&mesh.positions, face[1] as usize);
            let c = index_vertex(&mesh.positions, face[2] as usize);
            triangles.push(SourceTriangle {
                vertices: [
                    Vec3::new(a[0], -a[1], a[2]),
                    Vec3::new(b[0], -b[1], b[2]),
                    Vec3::new(c[0], -c[1], c[2]),
                ],
                color: AppColor::from_rgb(190, 190, 190),
                alpha: 255,
            });
        }
    }

    Ok(Mesh { triangles })
}

fn load_stl_mesh(path: &str) -> Result<Mesh> {
    let file = File::open(path).with_context(|| format!("Failed to open STL file: {path}"))?;
    let mut reader = BufReader::new(file);
    let mesh = stl_io::read_stl(&mut reader).with_context(|| format!("Failed to read STL file: {path}"))?;

    let mut triangles = Vec::new();
    for face in mesh.faces {
        let a = mesh.vertices[face.vertices[0]];
        let b = mesh.vertices[face.vertices[1]];
        let c = mesh.vertices[face.vertices[2]];
        triangles.push(SourceTriangle {
            vertices: [
                Vec3::new(a[0], -a[1], a[2]),
                Vec3::new(b[0], -b[1], b[2]),
                Vec3::new(c[0], -c[1], c[2]),
            ],
            color: AppColor::from_rgb(190, 190, 190),
            alpha: 255,
        });
    }

    Ok(Mesh { triangles })
}

fn fox_buddy_animated_mesh(target_size: f32, elapsed_seconds: f64, velocity: Vector2) -> Mesh {
    let Some(asset) = fox_asset() else {
        return fox_buddy_mesh(target_size);
    };
    let speed = velocity.x.abs() + velocity.y.abs();
    let preferred_clip = if speed > 240.0 {
        "Run"
    } else if speed > 30.0 {
        "Walk"
    } else {
        "Survey"
    };
    let animation = asset
        .animations
        .iter()
        .find(|animation| animation.name == preferred_clip)
        .or_else(|| asset.animations.first());
    let mut local_translations: Vec<Vec3> = asset.nodes.iter().map(|node| node.base_translation).collect();
    let mut local_rotations: Vec<Quat> = asset.nodes.iter().map(|node| node.base_rotation).collect();
    let mut local_scales: Vec<Vec3> = asset.nodes.iter().map(|node| node.base_scale).collect();

    if let Some(animation) = animation {
        let time = if animation.duration > 0.0 {
            (elapsed_seconds as f32 * if preferred_clip == "Survey" { 0.55 } else { 1.0 }) % animation.duration
        } else {
            0.0
        };
        for channel in &animation.channels {
            match (&channel.property, &channel.values) {
                (FoxAnimatedProperty::Translation, FoxAnimationValues::Vec3(values)) => {
                    local_translations[channel.node_index] = sample_vec3(&channel.times, values, time);
                },
                (FoxAnimatedProperty::Rotation, FoxAnimationValues::Rotation(values)) => {
                    local_rotations[channel.node_index] = sample_quat(&channel.times, values, time);
                },
                (FoxAnimatedProperty::Scale, FoxAnimationValues::Vec3(values)) => {
                    local_scales[channel.node_index] = sample_vec3(&channel.times, values, time);
                },
                _ => {},
            }
        }
    }

    let mut local_matrices = Vec::with_capacity(asset.nodes.len());
    for index in 0..asset.nodes.len() {
        local_matrices.push(Mat4::from_trs(local_translations[index], local_rotations[index], local_scales[index]));
    }
    let globals = compute_global_matrices(&asset.nodes, &local_matrices);
    let joint_matrices: Vec<Mat4> = asset
        .skin_joints
        .iter()
        .enumerate()
        .map(|(joint_index, &node_index)| globals[node_index].mul(asset.inverse_bind_matrices[joint_index]))
        .collect();

    let mut skinned_positions = Vec::with_capacity(asset.vertices.len());
    for vertex in &asset.vertices {
        let mut skinned = Vec3::new(0.0, 0.0, 0.0);
        let mut total_weight = 0.0;
        for influence in 0..4 {
            let weight = vertex.weights[influence];
            let Some(matrix) = joint_matrices.get(vertex.joints[influence]) else {
                continue;
            };
            if weight <= 0.0 {
                continue;
            }
            let transformed = matrix.transform_point(vertex.position);
            skinned.x += transformed.x * weight;
            skinned.y += transformed.y * weight;
            skinned.z += transformed.z * weight;
            total_weight += weight;
        }
        if total_weight <= f32::EPSILON {
            skinned = vertex.position;
        }
        skinned_positions.push(Vec3::new(skinned.x, -skinned.y, skinned.z));
    }

    let mut mesh = Mesh {
        triangles: asset
            .indices
            .iter()
            .filter_map(|face| {
                Some(SourceTriangle {
                    vertices: [
                        *skinned_positions.get(face[0])?,
                        *skinned_positions.get(face[1])?,
                        *skinned_positions.get(face[2])?,
                    ],
                    color: asset.color,
                    alpha: asset.color.a,
                })
            })
            .collect(),
    };
    normalize_mesh(&mut mesh, target_size).unwrap_or(());
    mesh
}

fn fox_buddy_mesh(target_size: f32) -> Mesh {
    static FOX_GLB: &[u8] = include_bytes!("../../../Assets/characters/fox/Fox.glb");
    match load_gltf_mesh_from_slice(FOX_GLB, target_size, AppColor::from_rgb(245, 126, 48)) {
        Ok(mesh) => mesh,
        Err(_) => ball_mesh(target_size, AppColor::from_rgb(245, 126, 48)),
    }
}

fn fox_asset() -> Option<&'static FoxAsset> {
    static FOX_ASSET: OnceLock<Option<FoxAsset>> = OnceLock::new();
    FOX_ASSET
        .get_or_init(|| load_fox_asset().ok())
        .as_ref()
}

fn load_fox_asset() -> Result<FoxAsset> {
    static FOX_GLB: &[u8] = include_bytes!("../../../Assets/characters/fox/Fox.glb");
    let (document, buffers, _) = gltf::import_slice(FOX_GLB).context("Failed to import fox GLB")?;
    let buffer_data = |buffer: gltf::Buffer<'_>| buffers.get(buffer.index()).map(|data| data.0.as_slice());
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut color = AppColor::from_rgb(245, 126, 48);

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(buffer_data);
            let Some(positions_reader) = reader.read_positions() else {
                continue;
            };
            let base_index = vertices.len();
            let positions: Vec<[f32; 3]> = positions_reader.collect();
            let joints: Vec<[u16; 4]> = reader
                .read_joints(0)
                .map(ReadJoints::into_u16)
                .map(|values| values.collect())
                .unwrap_or_else(|| vec![[0, 0, 0, 0]; positions.len()]);
            let weights: Vec<[f32; 4]> = reader
                .read_weights(0)
                .map(ReadWeights::into_f32)
                .map(|values| values.collect())
                .unwrap_or_else(|| vec![[1.0, 0.0, 0.0, 0.0]; positions.len()]);
            let material = primitive.material();
            let color_factor = material.pbr_metallic_roughness().base_color_factor();
            color = AppColor::from_argb(
                (color_factor[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                scale_channel(245, color_factor[0]),
                scale_channel(126, color_factor[1]),
                scale_channel(48, color_factor[2]),
            );

            for (index, position) in positions.iter().enumerate() {
                vertices.push(FoxVertex {
                    position: Vec3::new(position[0], position[1], position[2]),
                    joints: [
                        joints.get(index).copied().unwrap_or_default()[0] as usize,
                        joints.get(index).copied().unwrap_or_default()[1] as usize,
                        joints.get(index).copied().unwrap_or_default()[2] as usize,
                        joints.get(index).copied().unwrap_or_default()[3] as usize,
                    ],
                    weights: weights.get(index).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]),
                });
            }

            let primitive_indices: Vec<usize> = match reader.read_indices() {
                Some(ReadIndices::U8(values)) => values.map(|value| base_index + value as usize).collect(),
                Some(ReadIndices::U16(values)) => values.map(|value| base_index + value as usize).collect(),
                Some(ReadIndices::U32(values)) => values.map(|value| base_index + value as usize).collect(),
                None => (base_index..base_index + positions.len()).collect(),
            };
            for face in primitive_indices.chunks_exact(3) {
                indices.push([face[0], face[1], face[2]]);
            }
        }
    }

    let nodes: Vec<FoxNode> = document
        .nodes()
        .map(|node| {
            let (translation, rotation, scale) = node.transform().decomposed();
            FoxNode {
                children: node.children().map(|child| child.index()).collect(),
                base_translation: Vec3::new(translation[0], translation[1], translation[2]),
                base_rotation: Quat::new(rotation[0], rotation[1], rotation[2], rotation[3]).normalized(),
                base_scale: Vec3::new(scale[0], scale[1], scale[2]),
            }
        })
        .collect();
    let skin = document.skins().next().context("Fox GLB did not contain a skin")?;
    let skin_reader = skin.reader(buffer_data);
    let skin_joints: Vec<usize> = skin.joints().map(|joint| joint.index()).collect();
    let inverse_bind_matrices: Vec<Mat4> = skin_reader
        .read_inverse_bind_matrices()
        .map(|matrices| matrices.map(Mat4::from_gltf_matrix).collect())
        .unwrap_or_else(|| vec![Mat4::identity(); skin_joints.len()]);
    let animations = load_fox_animations(&document, &buffers);

    Ok(FoxAsset {
        vertices,
        indices,
        color,
        nodes,
        skin_joints,
        inverse_bind_matrices,
        animations,
    })
}

fn load_fox_animations(document: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Vec<FoxAnimation> {
    let mut animations = Vec::new();
    for animation in document.animations() {
        let mut channels = Vec::new();
        let mut duration = 0.0f32;
        for channel in animation.channels() {
            let reader = channel.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
            let Some(times) = reader.read_inputs().map(|values| values.collect::<Vec<_>>()) else {
                continue;
            };
            if let Some(last) = times.last() {
                duration = duration.max(*last);
            }
            let Some(outputs) = reader.read_outputs() else {
                continue;
            };
            let node_index = channel.target().node().index();
            match outputs {
                ReadOutputs::Translations(values) => channels.push(FoxAnimationChannel {
                    node_index,
                    property: FoxAnimatedProperty::Translation,
                    times,
                    values: FoxAnimationValues::Vec3(values.map(|value| Vec3::new(value[0], value[1], value[2])).collect()),
                }),
                ReadOutputs::Rotations(values) => channels.push(FoxAnimationChannel {
                    node_index,
                    property: FoxAnimatedProperty::Rotation,
                    times,
                    values: FoxAnimationValues::Rotation(
                        values
                            .into_f32()
                            .map(|value| Quat::new(value[0], value[1], value[2], value[3]).normalized())
                            .collect(),
                    ),
                }),
                ReadOutputs::Scales(values) => channels.push(FoxAnimationChannel {
                    node_index,
                    property: FoxAnimatedProperty::Scale,
                    times,
                    values: FoxAnimationValues::Vec3(values.map(|value| Vec3::new(value[0], value[1], value[2])).collect()),
                }),
                ReadOutputs::MorphTargetWeights(_) => {},
            }
        }
        animations.push(FoxAnimation {
            name: animation.name().unwrap_or("Animation").to_string(),
            duration,
            channels,
        });
    }
    animations
}

fn sample_vec3(times: &[f32], values: &[Vec3], time: f32) -> Vec3 {
    if values.is_empty() {
        return Vec3::new(0.0, 0.0, 0.0);
    }
    let (left, right, blend) = sample_span(times, time);
    lerp_vec3(values[left.min(values.len() - 1)], values[right.min(values.len() - 1)], blend)
}

fn sample_quat(times: &[f32], values: &[Quat], time: f32) -> Quat {
    if values.is_empty() {
        return Quat::identity();
    }
    let (left, right, blend) = sample_span(times, time);
    values[left.min(values.len() - 1)]
        .slerp(values[right.min(values.len() - 1)], blend)
        .normalized()
}

fn sample_span(times: &[f32], time: f32) -> (usize, usize, f32) {
    if times.len() <= 1 {
        return (0, 0, 0.0);
    }
    for index in 0..times.len() - 1 {
        let start = times[index];
        let end = times[index + 1];
        if time >= start && time <= end {
            let blend = if (end - start).abs() <= f32::EPSILON {
                0.0
            } else {
                (time - start) / (end - start)
            };
            return (index, index + 1, blend.clamp(0.0, 1.0));
        }
    }
    (times.len() - 1, times.len() - 1, 0.0)
}

fn compute_global_matrices(nodes: &[FoxNode], local_matrices: &[Mat4]) -> Vec<Mat4> {
    let mut has_parent = vec![false; nodes.len()];
    for node in nodes {
        for &child in &node.children {
            if let Some(slot) = has_parent.get_mut(child) {
                *slot = true;
            }
        }
    }

    let mut globals = vec![Mat4::identity(); nodes.len()];
    for index in 0..nodes.len() {
        if !has_parent[index] {
            fill_global_matrices(index, Mat4::identity(), nodes, local_matrices, &mut globals);
        }
    }
    globals
}

fn fill_global_matrices(index: usize, parent: Mat4, nodes: &[FoxNode], local_matrices: &[Mat4], globals: &mut [Mat4]) {
    let global = parent.mul(local_matrices[index]);
    globals[index] = global;
    for &child in &nodes[index].children {
        fill_global_matrices(child, global, nodes, local_matrices, globals);
    }
}

fn lerp_vec3(left: Vec3, right: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        left.x + (right.x - left.x) * t,
        left.y + (right.y - left.y) * t,
        left.z + (right.z - left.z) * t,
    )
}

fn load_gltf_mesh_from_slice(bytes: &[u8], target_size: f32, fallback_color: AppColor) -> Result<Mesh> {
    let (document, buffers, _) = gltf::import_slice(bytes).context("Failed to import GLB model")?;
    let mut mesh = Mesh {
        triangles: Vec::new(),
    };

    for scene in document.scenes() {
        for node in scene.nodes() {
            append_gltf_node(&mut mesh, &node, &buffers, identity_matrix(), fallback_color)?;
        }
    }

    if mesh.triangles.is_empty() {
        bail!("GLB model contained no triangles.");
    }

    limit_imported_mesh(&mut mesh);
    normalize_mesh(&mut mesh, target_size)?;
    Ok(mesh)
}

fn append_gltf_node(
    mesh: &mut Mesh,
    node: &gltf::Node<'_>,
    buffers: &[gltf::buffer::Data],
    parent_transform: [[f32; 4]; 4],
    fallback_color: AppColor,
) -> Result<()> {
    let transform = multiply_matrix(parent_transform, node.transform().matrix());
    if let Some(node_mesh) = node.mesh() {
        for primitive in node_mesh.primitives() {
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
            let Some(positions) = reader.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            let indices: Vec<u32> = match reader.read_indices() {
                Some(ReadIndices::U8(values)) => values.map(u32::from).collect(),
                Some(ReadIndices::U16(values)) => values.map(u32::from).collect(),
                Some(ReadIndices::U32(values)) => values.collect(),
                None => (0..positions.len() as u32).collect(),
            };

            let material = primitive.material();
            let color_factor = material.pbr_metallic_roughness().base_color_factor();
            let color = AppColor::from_argb(
                (color_factor[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                multiply_channel(fallback_color.r, (color_factor[0] * 255.0).round().clamp(0.0, 255.0) as u8),
                multiply_channel(fallback_color.g, (color_factor[1] * 255.0).round().clamp(0.0, 255.0) as u8),
                multiply_channel(fallback_color.b, (color_factor[2] * 255.0).round().clamp(0.0, 255.0) as u8),
            );

            for face in indices.chunks_exact(3) {
                let a = positions[face[0] as usize];
                let b = positions[face[1] as usize];
                let c = positions[face[2] as usize];
                mesh.triangles.push(SourceTriangle {
                    vertices: [
                        transform_gltf_vertex(a, transform),
                        transform_gltf_vertex(b, transform),
                        transform_gltf_vertex(c, transform),
                    ],
                    color,
                    alpha: color.a,
                });
            }
        }
    }

    for child in node.children() {
        append_gltf_node(mesh, &child, buffers, transform, fallback_color)?;
    }

    Ok(())
}

fn identity_matrix() -> [[f32; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn multiply_matrix(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut result = [[0.0; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] = left[row][0] * right[0][column]
                + left[row][1] * right[1][column]
                + left[row][2] * right[2][column]
                + left[row][3] * right[3][column];
        }
    }
    result
}

fn transform_gltf_vertex(vertex: [f32; 3], transform: [[f32; 4]; 4]) -> Vec3 {
    let x = vertex[0];
    let y = vertex[1];
    let z = vertex[2];
    Vec3::new(
        transform[0][0] * x + transform[0][1] * y + transform[0][2] * z + transform[0][3],
        -(transform[1][0] * x + transform[1][1] * y + transform[1][2] * z + transform[1][3]),
        transform[2][0] * x + transform[2][1] * y + transform[2][2] * z + transform[2][3],
    )
}

fn limit_imported_mesh(mesh: &mut Mesh) {
    let triangle_count = mesh.triangles.len();
    if triangle_count <= MAX_IMPORTED_TRIANGLES {
        return;
    }

    let mut limited = Vec::with_capacity(MAX_IMPORTED_TRIANGLES);
    for index in 0..MAX_IMPORTED_TRIANGLES {
        let source_index = index * triangle_count / MAX_IMPORTED_TRIANGLES;
        limited.push(mesh.triangles[source_index]);
    }
    mesh.triangles = limited;
}

fn normalize_mesh(mesh: &mut Mesh, target_size: f32) -> Result<()> {
    let mut min = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut max = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for triangle in &mesh.triangles {
        for vertex in triangle.vertices {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            min.z = min.z.min(vertex.z);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
            max.z = max.z.max(vertex.z);
        }
    }

    let size_x = max.x - min.x;
    let size_y = max.y - min.y;
    let size_z = max.z - min.z;
    let collision_footprint = size_x.max(size_y);
    if collision_footprint <= f32::EPSILON {
        bail!("Imported model has zero size.");
    }

    let center = Vec3::new(min.x + size_x * 0.5, min.y + size_y * 0.5, min.z + size_z * 0.5);
    let scale = target_size / collision_footprint;
    for triangle in &mut mesh.triangles {
        for vertex in &mut triangle.vertices {
            vertex.x = (vertex.x - center.x) * scale;
            vertex.y = (vertex.y - center.y) * scale;
            vertex.z = (vertex.z - center.z) * scale;
        }
    }

    Ok(())
}

fn index_vertex(data: &[f32], index: usize) -> [f32; 3] {
    let base = index * 3;
    [data[base], data[base + 1], data[base + 2]]
}
