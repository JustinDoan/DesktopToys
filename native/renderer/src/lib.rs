use std::{collections::HashMap, fs::File, io::BufReader, mem, path::Path};

use anyhow::{bail, Context, Result};
use core_types::{AppColor, ObjectState, ObjectVisualKind, RectF, ScreenShardGeometry, Vector2};
use font8x8::UnicodeFonts;

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
    pub center_message: Option<String>,
    pub panels: Vec<OverlayPanel>,
}

pub struct RenderScene<'a> {
    pub bounds: RectF,
    pub elapsed_seconds: f64,
    pub objects: &'a [ObjectState],
<<<<<<< Updated upstream
    pub cursor: Vector2,
=======
    pub shatter_backdrop_active: bool,
    pub sand_cells: &'a [SandRenderCell],
    pub weather_cells: &'a [SandRenderCell],
    pub measure_cells: &'a [SandRenderCell],
    pub spotlight_cells: &'a [SandRenderCell],
    pub lasso_cells: &'a [SandRenderCell],
    pub portal_cells: &'a [SandRenderCell],
    pub shatter_gun_cells: &'a [SandRenderCell],
>>>>>>> Stashed changes
    pub hud: &'a HudState,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GpuVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub material: [f32; 4],
    pub material_extra: [f32; 4],
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

#[derive(Clone, Copy, Debug)]
struct DrawTriangle {
    points: [Vec3; 3],
    color: AppColor,
    alpha: u8,
}

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
<<<<<<< Updated upstream
        vertices.reserve(scene.objects.len() * 36 + 4096);
        for object in scene.objects {
=======
        vertices.reserve(
            scene.objects.len() * 36
                + (scene.sand_cells.len()
                    + scene.weather_cells.len()
                    + scene.measure_cells.len()
                    + scene.spotlight_cells.len()
                    + scene.lasso_cells.len()
                    + scene.portal_cells.len()
                    + scene.shatter_gun_cells.len())
                    * 6
                + 4096,
        );
        if scene.shatter_backdrop_active {
            emit_shatter_backdrop(&mut vertices, width, height);
        }
        for object in scene.objects {
            if object.visual_kind == ObjectVisualKind::ScreenShard {
                if let Some(shard) = &object.screen_shard {
                    let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
                    emit_screen_shard(&mut vertices, shard, transform);
                }
                continue;
            }

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

            if is_shader_sphere(object.visual_kind) {
                let mesh = self.mesh_for_object(object)?;
                let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
                let material_id = shader_sphere_material_id(object.visual_kind);
                for triangle in &mesh.triangles {
                    emit_shader_sphere_triangle(&mut vertices, *triangle, transform, material_id, scene.elapsed_seconds as f32);
                }
                continue;
            }

            if is_shader_cube(object.visual_kind) {
                let mesh = self.mesh_for_object(object)?;
                let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
                for triangle in &mesh.triangles {
                    emit_shader_cube_triangle(&mut vertices, *triangle, transform, 6.0, scene.elapsed_seconds as f32);
                }
                continue;
            }

>>>>>>> Stashed changes
            let mesh = self.mesh_for_object(object)?;
            let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
            for triangle in &mesh.triangles {
                emit_draw_triangle(
                    &mut vertices,
                    transform_triangle(*triangle, transform, scene.elapsed_seconds),
                );
            }
        }

<<<<<<< Updated upstream
        emit_cursor(&mut vertices, width, height, scene.cursor);
=======
        emit_sand(&mut vertices, width, height, scene.sand_cells);
        emit_sand(&mut vertices, width, height, scene.weather_cells);
        emit_sand(&mut vertices, width, height, scene.spotlight_cells);
        emit_sand(&mut vertices, width, height, scene.measure_cells);
        emit_sand(&mut vertices, width, height, scene.lasso_cells);
        emit_sand(&mut vertices, width, height, scene.portal_cells);
        emit_sand(&mut vertices, width, height, scene.shatter_gun_cells);
>>>>>>> Stashed changes
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
<<<<<<< Updated upstream
=======
            ObjectVisualKind::Ball => self.generated_mesh(key, || ball_mesh(size, object.base_color)),
            ObjectVisualKind::SoftBall => self.generated_mesh(key, || soft_ball_mesh(size, object.base_color)),
            ObjectVisualKind::GlassMarble => self.generated_mesh(key, || glass_marble_mesh(size, object.base_color)),
            ObjectVisualKind::PlasmaOrb => self.generated_mesh(key, || glass_marble_mesh(size, object.base_color)),
            ObjectVisualKind::PortalOrb => self.generated_mesh(key, || glass_marble_mesh(size, object.base_color)),
            ObjectVisualKind::SoapBubble => self.generated_mesh(key, || glass_marble_mesh(size, object.base_color)),
            ObjectVisualKind::ForcefieldOrb => self.generated_mesh(key, || glass_marble_mesh(size, object.base_color)),
            ObjectVisualKind::RaymarchCube => self.generated_mesh(key, || cube_mesh(size, object.base_color)),
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
            ObjectVisualKind::Snail => self.generated_mesh(key, || snail_mesh(size, object.base_color)),
            ObjectVisualKind::Fan => self.generated_mesh(key, || fan_mesh(size, object.base_color)),
            ObjectVisualKind::QuadDrone => self.generated_mesh(key, || quad_drone_mesh(size, object.base_color)),
            ObjectVisualKind::Basketball => self.generated_mesh(key, || basketball_mesh(size)),
            ObjectVisualKind::BasketballHoop => self.generated_mesh(key, || {
                basketball_hoop_mesh(object.body.width.max(1.0), object.body.height.max(1.0))
            }),
            ObjectVisualKind::ScreenShard => self.generated_mesh(key, || {
                rectangular_prism_mesh(object.body.width.max(1.0), object.body.height.max(1.0), 18.0, object.base_color)
            }),
>>>>>>> Stashed changes
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
<<<<<<< Updated upstream
                ObjectVisualKind::ImportedModel => 5,
=======
                ObjectVisualKind::Ball => 5,
                ObjectVisualKind::SoftBall => 6,
                ObjectVisualKind::GlassMarble => 7,
                ObjectVisualKind::PlasmaOrb => 8,
                ObjectVisualKind::PortalOrb => 9,
                ObjectVisualKind::SoapBubble => 10,
                ObjectVisualKind::ForcefieldOrb => 11,
                ObjectVisualKind::RaymarchCube => 12,
                ObjectVisualKind::Pyramid => 13,
                ObjectVisualKind::Barrel => 14,
                ObjectVisualKind::Ring => 15,
                ObjectVisualKind::Star => 16,
                ObjectVisualKind::GamePlank => 17,
                ObjectVisualKind::GameTarget => 18,
                ObjectVisualKind::FoxBuddy => 19,
                ObjectVisualKind::RobotBuddy => 20,
                ObjectVisualKind::Snail => 21,
                ObjectVisualKind::Fan => 22,
                ObjectVisualKind::QuadDrone => 23,
                ObjectVisualKind::ImportedModel => 24,
                ObjectVisualKind::Basketball => 25,
                ObjectVisualKind::BasketballHoop => 26,
                ObjectVisualKind::ScreenShard => 27,
>>>>>>> Stashed changes
            },
            width_milli: quantize_size(object.body.width.max(1.0)),
            height_milli: quantize_size(object.body.height.max(1.0)),
            size_milli: quantize_size(size),
            color: object.base_color,
        }
    }
}

fn is_shader_sphere(visual_kind: ObjectVisualKind) -> bool {
    matches!(
        visual_kind,
        ObjectVisualKind::GlassMarble
            | ObjectVisualKind::PlasmaOrb
            | ObjectVisualKind::PortalOrb
            | ObjectVisualKind::SoapBubble
            | ObjectVisualKind::ForcefieldOrb
    )
}

fn is_shader_cube(visual_kind: ObjectVisualKind) -> bool {
    matches!(visual_kind, ObjectVisualKind::RaymarchCube)
}

fn shader_sphere_material_id(visual_kind: ObjectVisualKind) -> f32 {
    match visual_kind {
        ObjectVisualKind::GlassMarble => 1.0,
        ObjectVisualKind::PlasmaOrb => 2.0,
        ObjectVisualKind::PortalOrb => 3.0,
        ObjectVisualKind::SoapBubble => 4.0,
        ObjectVisualKind::ForcefieldOrb => 5.0,
        _ => 0.0,
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

    if let Some(message) = &hud.center_message {
        emit_center_message(vertices, width, height, message);
    }
}

<<<<<<< Updated upstream
=======
fn emit_sand(vertices: &mut Vec<GpuVertex>, width: u32, height: u32, cells: &[SandRenderCell]) {
    for cell in cells {
        emit_rect(vertices, width, height, cell.x, cell.y, cell.width, cell.height, cell.color, 0.028);
    }
}

fn emit_shatter_backdrop(vertices: &mut Vec<GpuVertex>, width: u32, height: u32) {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let camera_distance = width.max(height) * 1.25;
    let z = -camera_distance;
    let perspective = camera_distance / (camera_distance - z);
    let center_x = width * 0.5;
    let center_y = height * 0.5;
    let left = center_x + (0.0 - center_x) / perspective;
    let right = center_x + (width - center_x) / perspective;
    let top = center_y + (0.0 - center_y) / perspective;
    let bottom = center_y + (height - center_y) / perspective;
    let color = color_to_f32(AppColor::from_rgb(43, 43, 45), 255);
    let material = [0.0, 0.0, 0.0, 0.0];
    let material_extra = [0.0, 0.0, 0.0, 0.0];

    vertices.extend_from_slice(&[
        GpuVertex {
            position: [left, top, z],
            color,
            material,
            material_extra,
        },
        GpuVertex {
            position: [right, top, z],
            color,
            material,
            material_extra,
        },
        GpuVertex {
            position: [right, bottom, z],
            color,
            material,
            material_extra,
        },
        GpuVertex {
            position: [left, top, z],
            color,
            material,
            material_extra,
        },
        GpuVertex {
            position: [right, bottom, z],
            color,
            material,
            material_extra,
        },
        GpuVertex {
            position: [left, bottom, z],
            color,
            material,
            material_extra,
        },
    ]);
}

>>>>>>> Stashed changes
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
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        });
    }
}

fn emit_shader_sphere_triangle(
    vertices: &mut Vec<GpuVertex>,
    triangle: SourceTriangle,
    transform: VisualTransform,
    material_id: f32,
    elapsed_seconds: f32,
) {
    let points = triangle.vertices.map(|vertex| project_vertex(vertex, transform));
    let color = color_to_f32(triangle.color, triangle.alpha);
    for (index, point) in points.into_iter().enumerate() {
        let marble_point = triangle.vertices[index].normalized();
        let normal = rotate_normal(marble_point, transform);
        vertices.push(GpuVertex {
            position: [point.x, point.y, point.z],
            color,
            material: [material_id, marble_point.x, marble_point.y, marble_point.z],
            material_extra: [normal.x, normal.y, normal.z, elapsed_seconds],
        });
    }
}

fn emit_shader_cube_triangle(
    vertices: &mut Vec<GpuVertex>,
    triangle: SourceTriangle,
    transform: VisualTransform,
    material_id: f32,
    elapsed_seconds: f32,
) {
    let points = triangle.vertices.map(|vertex| project_vertex(vertex, transform));
    let color = color_to_f32(triangle.color, triangle.alpha);
    let edge_a = Vec3::new(
        triangle.vertices[1].x - triangle.vertices[0].x,
        triangle.vertices[1].y - triangle.vertices[0].y,
        triangle.vertices[1].z - triangle.vertices[0].z,
    );
    let edge_b = Vec3::new(
        triangle.vertices[2].x - triangle.vertices[0].x,
        triangle.vertices[2].y - triangle.vertices[0].y,
        triangle.vertices[2].z - triangle.vertices[0].z,
    );
    let local_normal = edge_a.cross(edge_b).normalized();
    let normal = rotate_normal(local_normal, transform);
    for (index, point) in points.into_iter().enumerate() {
        let cube_point = cube_material_point(triangle.vertices[index]);
        vertices.push(GpuVertex {
            position: [point.x, point.y, point.z],
            color,
            material: [material_id, cube_point.x, cube_point.y, cube_point.z],
            material_extra: [normal.x, normal.y, normal.z, elapsed_seconds],
        });
    }
}

fn emit_center_message(vertices: &mut Vec<GpuVertex>, width: u32, height: u32, message: &str) {
    let scale = 5;
    let text_width = message.chars().count() as i32 * 8 * scale;
    let text_height = 8 * scale;
    let panel_width = text_width + 54;
    let panel_height = text_height + 34;
    let left = (width as i32 - panel_width) / 2;
    let top = (height as i32 - panel_height) / 2;
    emit_rect(
        vertices,
        width,
        height,
        left,
        top,
        panel_width,
        panel_height,
        AppColor::from_argb(218, 8, 8, 10),
        0.08,
    );
    emit_rect_outline(
        vertices,
        width,
        height,
        left,
        top,
        panel_width,
        panel_height,
        AppColor::from_argb(255, 255, 82, 82),
        0.07,
    );
    emit_text(
        vertices,
        width,
        height,
        left + 27,
        top + 17,
        message,
        AppColor::from_rgb(255, 82, 82),
        scale,
        0.055,
    );
}

const SCREEN_SHARD_MATERIAL_ID: f32 = 7.0;

fn emit_screen_shard(vertices: &mut Vec<GpuVertex>, shard: &ScreenShardGeometry, transform: VisualTransform) {
    let point_count = shard.local_points.len().min(shard.texture_uvs.len());
    if point_count < 3 {
        return;
    }

    let hz = shard.thickness.max(2.0) * 0.5;
    let front: Vec<Vec3> = shard
        .local_points
        .iter()
        .take(point_count)
        .map(|point| Vec3::new(point.x, point.y, hz))
        .collect();
    let back: Vec<Vec3> = shard
        .local_points
        .iter()
        .take(point_count)
        .map(|point| Vec3::new(point.x, point.y, -hz))
        .collect();
    let front_normal = rotate_normal(Vec3::new(0.0, 0.0, 1.0), transform);
    let back_normal = rotate_normal(Vec3::new(0.0, 0.0, -1.0), transform);

    for index in 1..point_count - 1 {
        emit_textured_shard_triangle(
            vertices,
            [front[0], front[index], front[index + 1]],
            [
                shard.texture_uvs[0],
                shard.texture_uvs[index],
                shard.texture_uvs[index + 1],
            ],
            transform,
            front_normal,
        );
        emit_solid_local_triangle(
            vertices,
            [back[0], back[index + 1], back[index]],
            transform,
            AppColor::from_rgb(38, 41, 45),
            back_normal,
            255,
        );
    }

    for index in 0..point_count {
        let next = (index + 1) % point_count;
        let a = front[index];
        let b = front[next];
        let c = back[next];
        let d = back[index];
        let normal = local_triangle_normal([a, b, c]);
        emit_solid_local_triangle(
            vertices,
            [a, b, c],
            transform,
            AppColor::from_rgb(54, 58, 63),
            rotate_normal(normal, transform),
            255,
        );
        emit_solid_local_triangle(
            vertices,
            [a, c, d],
            transform,
            AppColor::from_rgb(43, 46, 50),
            rotate_normal(normal, transform),
            255,
        );
    }
}

fn emit_textured_shard_triangle(
    vertices: &mut Vec<GpuVertex>,
    points: [Vec3; 3],
    uvs: [Vector2; 3],
    transform: VisualTransform,
    normal: Vec3,
) {
    for index in 0..3 {
        let point = project_vertex(points[index], transform);
        let uv = uvs[index];
        vertices.push(GpuVertex {
            position: [point.x, point.y, point.z],
            color: [1.0, 1.0, 1.0, 1.0],
            material: [SCREEN_SHARD_MATERIAL_ID, uv.x, uv.y, 0.0],
            material_extra: [normal.x, normal.y, normal.z, 0.0],
        });
    }
}

fn emit_solid_local_triangle(
    vertices: &mut Vec<GpuVertex>,
    points: [Vec3; 3],
    transform: VisualTransform,
    color: AppColor,
    normal: Vec3,
    alpha: u8,
) {
    let projected = points.map(|point| project_vertex(point, transform));
    let shade = (0.62 + normal.z.max(0.0) * 0.28 + (-normal.y).max(0.0) * 0.12).clamp(0.42, 1.0);
    let color = color_to_f32(scale_color(color, shade), alpha);
    for point in projected {
        vertices.push(GpuVertex {
            position: [point.x, point.y, point.z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        });
    }
}

fn local_triangle_normal(points: [Vec3; 3]) -> Vec3 {
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
    edge_a.cross(edge_b).normalized()
}

fn cube_material_point(point: Vec3) -> Vec3 {
    let scale = point.x.abs().max(point.y.abs()).max(point.z.abs()).max(1.0);
    Vec3::new(point.x / scale, point.y / scale, point.z / scale)
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
<<<<<<< Updated upstream
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
=======
    let color = color_to_f32(color, color.a);
    vertices.extend_from_slice(&[
        GpuVertex {
            position: [left, top, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        },
        GpuVertex {
            position: [right, top, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        },
        GpuVertex {
            position: [right, bottom, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        },
        GpuVertex {
            position: [left, top, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        },
        GpuVertex {
            position: [right, bottom, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
        },
        GpuVertex {
            position: [left, bottom, z],
            color,
            material: [0.0, 0.0, 0.0, 0.0],
            material_extra: [0.0, 0.0, 0.0, 0.0],
>>>>>>> Stashed changes
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
    let mut rotation_x = object.rotation_x;
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
<<<<<<< Updated upstream
        ObjectVisualKind::ImportedModel => {
            center_z += 12.0;
        },
=======
        ObjectVisualKind::Ball => {
            center_z += 10.0;
        },
        ObjectVisualKind::SoftBall => {
            let speed = object.body.velocity.length_squared().sqrt();
            let wobble = ((elapsed_seconds * 10.5) + phase).sin() as f32;
            let impact = soft_impact_amount(object, bounds);
            let stretch = (speed / 1900.0).min(1.0);
            center_z += 9.0 + wobble * (1.0 + stretch * 3.0);
            scale_x *= 1.0 + stretch * 0.13 + impact * 0.18 + wobble * 0.025;
            scale_y *= 1.0 - stretch * 0.05 - impact * 0.22 - wobble * 0.018;
            rotation_z += (object.body.velocity.x as f64 * 0.012).clamp(-8.0, 8.0);
        },
        ObjectVisualKind::GlassMarble
        | ObjectVisualKind::PlasmaOrb
        | ObjectVisualKind::PortalOrb
        | ObjectVisualKind::SoapBubble
        | ObjectVisualKind::ForcefieldOrb => {
            center_z += 12.0;
            rotation_z += object.body.velocity.x as f64 * 0.009;
            rotation_x += object.body.velocity.y as f64 * 0.004;
        },
        ObjectVisualKind::RaymarchCube => {
            center_z += 8.0;
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
        ObjectVisualKind::Snail => {
            let speed_sq = object.body.velocity.length_squared();
            if speed_sq > 1.0 {
                rotation_z += object.body.velocity.y.atan2(object.body.velocity.x).to_degrees() as f64;
            }
            let crawl = ((elapsed_seconds * 5.2) + phase).sin() as f32;
            center_z += 8.0 + crawl * 1.8;
            scale_x *= 1.0 + crawl * 0.018;
            scale_y *= 1.0 - crawl * 0.012;
        },
        ObjectVisualKind::Fan => {
            let pulse = ((elapsed_seconds * 8.0) + phase).sin() as f32;
            center_z += 15.0 + pulse.max(0.0) * 2.0;
            scale_x *= 1.0 + pulse * 0.01;
            scale_y *= 1.0 - pulse * 0.006;
        },
        ObjectVisualKind::QuadDrone => {
            let hover = ((elapsed_seconds * 4.2) + phase).sin() as f32;
            center_z += 38.0 + hover * 5.0;
            rotation_x += (object.body.velocity.y as f64 * 0.018).clamp(-9.0, 9.0);
            rotation_y += (-object.body.velocity.x as f64 * 0.018).clamp(-9.0, 9.0);
        },
        ObjectVisualKind::ImportedModel => {
            center_z += 12.0;
        },
        ObjectVisualKind::Basketball => {
            center_z += 10.0;
        },
        ObjectVisualKind::BasketballHoop => {},
        ObjectVisualKind::ScreenShard => {},
>>>>>>> Stashed changes
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

    DrawTriangle {
        points,
        color: source.color,
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

fn rotate_normal(normal: Vec3, transform: VisualTransform) -> Vec3 {
    let mut point = rotate_x(normal, transform.rotation_x as f32);
    point = rotate_y(point, transform.rotation_y as f32);
    point = rotate_z(point, transform.rotation_z as f32);
    point.normalized()
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

fn scale_channel(value: u8, factor: f32) -> u8 {
    ((value as f32 * factor).round()).clamp(0.0, 255.0) as u8
}

fn cube_mesh(size: f32, base_color: AppColor) -> Mesh {
    let hs = size * 0.5;
    let faces = [
        (quad(Vec3::new(-hs, -hs, hs), Vec3::new(hs, -hs, hs), Vec3::new(hs, hs, hs), Vec3::new(-hs, hs, hs)), scale_color(base_color, 1.10)),
        (quad(Vec3::new(-hs, -hs, -hs), Vec3::new(-hs, hs, -hs), Vec3::new(hs, hs, -hs), Vec3::new(hs, -hs, -hs)), scale_color(base_color, 0.62)),
        (quad(Vec3::new(-hs, -hs, -hs), Vec3::new(-hs, -hs, hs), Vec3::new(-hs, hs, hs), Vec3::new(-hs, hs, -hs)), scale_color(base_color, 0.78)),
        (quad(Vec3::new(hs, -hs, -hs), Vec3::new(hs, hs, -hs), Vec3::new(hs, hs, hs), Vec3::new(hs, -hs, hs)), scale_color(base_color, 0.56)),
        (quad(Vec3::new(-hs, -hs, -hs), Vec3::new(hs, -hs, -hs), Vec3::new(hs, -hs, hs), Vec3::new(-hs, -hs, hs)), scale_color(base_color, 0.96)),
        (quad(Vec3::new(-hs, hs, -hs), Vec3::new(-hs, hs, hs), Vec3::new(hs, hs, hs), Vec3::new(hs, hs, -hs)), scale_color(base_color, 0.70)),
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

<<<<<<< Updated upstream
=======
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

fn ellipsoid_mesh(width: f32, height: f32, depth: f32, base_color: AppColor) -> Mesh {
    const LATITUDES: usize = 7;
    const LONGITUDES: usize = 14;
    let rx = width.max(1.0) * 0.5;
    let ry = height.max(1.0) * 0.5;
    let rz = depth.max(1.0) * 0.5;
    let mut triangles = Vec::new();

    for lat in 0..LATITUDES {
        let theta0 = std::f32::consts::PI * (lat as f32 / LATITUDES as f32);
        let theta1 = std::f32::consts::PI * ((lat + 1) as f32 / LATITUDES as f32);
        for lon in 0..LONGITUDES {
            let phi0 = std::f32::consts::TAU * (lon as f32 / LONGITUDES as f32);
            let phi1 = std::f32::consts::TAU * ((lon + 1) as f32 / LONGITUDES as f32);
            let p00 = ellipsoid_point(rx, ry, rz, theta0, phi0);
            let p01 = ellipsoid_point(rx, ry, rz, theta0, phi1);
            let p10 = ellipsoid_point(rx, ry, rz, theta1, phi0);
            let p11 = ellipsoid_point(rx, ry, rz, theta1, phi1);
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

fn ellipsoid_point(rx: f32, ry: f32, rz: f32, theta: f32, phi: f32) -> Vec3 {
    let sin_theta = theta.sin();
    Vec3::new(
        rx * sin_theta * phi.cos(),
        ry * theta.cos(),
        rz * sin_theta * phi.sin(),
    )
}

fn glass_marble_mesh(size: f32, base_color: AppColor) -> Mesh {
    const LATITUDES: usize = 28;
    const LONGITUDES: usize = 56;
    let radius = size * 0.5;
    let mut triangles = Vec::with_capacity(LATITUDES * LONGITUDES * 2);

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

            if lat == 0 {
                push_glass_marble_triangle(&mut triangles, [p00, p10, p11], radius, base_color);
            } else if lat + 1 == LATITUDES {
                push_glass_marble_triangle(&mut triangles, [p00, p10, p01], radius, base_color);
            } else {
                push_glass_marble_triangle(&mut triangles, [p00, p10, p11], radius, base_color);
                push_glass_marble_triangle(&mut triangles, [p00, p11, p01], radius, base_color);
            }
        }
    }

    Mesh { triangles }
}

fn push_glass_marble_triangle(triangles: &mut Vec<SourceTriangle>, vertices: [Vec3; 3], _radius: f32, base_color: AppColor) {
    triangles.push(SourceTriangle {
        vertices,
        color: base_color,
        alpha: 188,
    });
}

fn soft_ball_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut mesh = ball_mesh(size, base_color);
    let face_z = size * 0.43;
    let glow = scale_color(base_color, 1.24);
    let shade = scale_color(base_color, 0.58);
    add_disk(
        &mut mesh.triangles,
        Vec3::new(-size * 0.16, -size * 0.18, face_z + 1.2),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.16,
        glow,
        false,
    );
    add_disk(
        &mut mesh.triangles,
        Vec3::new(size * 0.18, size * 0.20, face_z + 0.7),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.12,
        shade,
        false,
    );
    mesh
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

/// Geometry of the basketball hoop mesh, in object-local units. The app uses
/// this for scoring so gameplay stays in sync with the rendered model.
/// Local axes match the mesh space: +y is down the screen, +z is toward the
/// viewer.
#[derive(Clone, Copy, Debug)]
pub struct HoopGeometry {
    pub backboard_width: f32,
    pub backboard_height: f32,
    pub backboard_center_y: f32,
    pub backboard_front_z: f32,
    pub rim_radius: f32,
    pub rim_center_y: f32,
    pub rim_center_z: f32,
}

pub fn hoop_geometry(width: f32, height: f32) -> HoopGeometry {
    let backboard_width = width * 0.92;
    let backboard_height = height * 0.52;
    let backboard_center_y = -height * 0.20;
    let backboard_front_z = -18.0;
    let rim_radius = width * 0.21;
    HoopGeometry {
        backboard_width,
        backboard_height,
        backboard_center_y,
        backboard_front_z,
        rim_radius,
        rim_center_y: backboard_center_y + (backboard_height * 0.5) - 4.0,
        rim_center_z: backboard_front_z + rim_radius + 8.0,
    }
}

fn basketball_mesh(size: f32) -> Mesh {
    let base_color = AppColor::from_rgb(235, 122, 48);
    let seam_color = AppColor::from_rgb(96, 48, 26);
    let mut mesh = ball_mesh(size, base_color);
    let seam_radius = size * 0.5 * 1.015;
    let seam_half_width = (size * 0.024).max(1.2);

    // Equator seam plus two vertical seams through the poles.
    let seams = [
        (Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, 1.0, 0.0)),
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 0.0)),
    ];
    for (axis_u, axis_v, normal) in seams {
        add_seam_band(
            &mut mesh.triangles,
            seam_radius,
            seam_half_width,
            axis_u,
            axis_v,
            normal,
            seam_color,
        );
    }

    mesh
}

fn add_seam_band(
    triangles: &mut Vec<SourceTriangle>,
    radius: f32,
    half_width: f32,
    axis_u: Vec3,
    axis_v: Vec3,
    normal: Vec3,
    color: AppColor,
) {
    const SEGMENTS: usize = 26;
    let circle_point = |angle: f32, side: f32| {
        let (sin, cos) = angle.sin_cos();
        Vec3::new(
            radius * (axis_u.x * cos + axis_v.x * sin) + normal.x * half_width * side,
            radius * (axis_u.y * cos + axis_v.y * sin) + normal.y * half_width * side,
            radius * (axis_u.z * cos + axis_v.z * sin) + normal.z * half_width * side,
        )
    };

    for i in 0..SEGMENTS {
        let a0 = std::f32::consts::TAU * (i as f32 / SEGMENTS as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / SEGMENTS as f32);
        let p00 = circle_point(a0, -1.0);
        let p01 = circle_point(a0, 1.0);
        let p10 = circle_point(a1, -1.0);
        let p11 = circle_point(a1, 1.0);
        triangles.push(SourceTriangle { vertices: [p00, p10, p11], color, alpha: 255 });
        triangles.push(SourceTriangle { vertices: [p00, p11, p01], color, alpha: 255 });
    }
}

fn basketball_hoop_mesh(width: f32, height: f32) -> Mesh {
    let geometry = hoop_geometry(width, height);
    let mut triangles = Vec::new();

    let board_color = AppColor::from_rgb(222, 229, 238);
    let frame_color = AppColor::from_rgb(112, 124, 138);
    let square_color = AppColor::from_rgb(255, 118, 44);
    let rim_color = AppColor::from_rgb(252, 88, 38);
    let net_color = AppColor::from_rgb(242, 246, 252);

    let board_thickness = 16.0;
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(
            geometry.backboard_width,
            geometry.backboard_height,
            board_thickness,
            board_color,
        ),
        Vec3::new(0.0, geometry.backboard_center_y, geometry.backboard_front_z - board_thickness * 0.5),
    );

    // Outer frame and shooter square painted as thin slabs on the front face.
    let face_z = geometry.backboard_front_z + 2.5;
    let frame_thickness = (width * 0.028).max(6.0);
    add_rect_outline_slabs(
        &mut triangles,
        geometry.backboard_width,
        geometry.backboard_height,
        Vec3::new(0.0, geometry.backboard_center_y, face_z),
        frame_thickness,
        frame_color,
    );
    let square_width = geometry.rim_radius * 2.15;
    let square_height = geometry.rim_radius * 1.5;
    add_rect_outline_slabs(
        &mut triangles,
        square_width,
        square_height,
        Vec3::new(0.0, geometry.rim_center_y - square_height * 0.62, face_z + 1.5),
        frame_thickness * 0.8,
        square_color,
    );

    // Rim: horizontal torus protruding from the board toward the viewer.
    const RIM_MAJOR_SEGMENTS: usize = 24;
    const RIM_MINOR_SEGMENTS: usize = 8;
    let rim_tube_radius = (width * 0.024).max(4.0);
    for i in 0..RIM_MAJOR_SEGMENTS {
        let a0 = std::f32::consts::TAU * (i as f32 / RIM_MAJOR_SEGMENTS as f32);
        let a1 = std::f32::consts::TAU * ((i + 1) as f32 / RIM_MAJOR_SEGMENTS as f32);
        for j in 0..RIM_MINOR_SEGMENTS {
            let b0 = std::f32::consts::TAU * (j as f32 / RIM_MINOR_SEGMENTS as f32);
            let b1 = std::f32::consts::TAU * ((j + 1) as f32 / RIM_MINOR_SEGMENTS as f32);
            let offset = Vec3::new(0.0, geometry.rim_center_y, geometry.rim_center_z);
            let point = |a: f32, b: f32| {
                let p = torus_point(geometry.rim_radius, rim_tube_radius, a, b);
                Vec3::new(p.x + offset.x, p.y + offset.y, p.z + offset.z)
            };
            let shade = 0.78 + (b0.cos().max(0.0) * 0.32);
            let color = scale_color(rim_color, shade);
            triangles.push(SourceTriangle {
                vertices: [point(a0, b0), point(a1, b0), point(a1, b1)],
                color,
                alpha: 255,
            });
            triangles.push(SourceTriangle {
                vertices: [point(a0, b0), point(a1, b1), point(a0, b1)],
                color,
                alpha: 255,
            });
        }
    }

    // Mount bracket connecting rim to the board.
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(geometry.rim_radius * 0.5, 8.0, geometry.rim_center_z - geometry.backboard_front_z, rim_color),
        Vec3::new(
            0.0,
            geometry.rim_center_y,
            geometry.backboard_front_z + (geometry.rim_center_z - geometry.backboard_front_z) * 0.5,
        ),
    );

    // Net: criss-crossing translucent strands tapering below the rim.
    const NET_STRANDS: usize = 12;
    let net_top_radius = geometry.rim_radius * 0.94;
    let net_bottom_radius = geometry.rim_radius * 0.52;
    let net_top_y = geometry.rim_center_y + 3.0;
    let net_bottom_y = geometry.rim_center_y + height * 0.20;
    let strand_arc = 0.085f32;
    let net_point = |angle: f32, radius: f32, y: f32| {
        Vec3::new(radius * angle.cos(), y, geometry.rim_center_z + radius * angle.sin())
    };
    for k in 0..NET_STRANDS {
        let top_angle = std::f32::consts::TAU * (k as f32 / NET_STRANDS as f32);
        for direction in [1.0f32, -1.0f32] {
            let bottom_angle = top_angle + direction * std::f32::consts::TAU / NET_STRANDS as f32;
            let t0 = net_point(top_angle, net_top_radius, net_top_y);
            let t1 = net_point(top_angle + strand_arc, net_top_radius, net_top_y);
            let b0 = net_point(bottom_angle, net_bottom_radius, net_bottom_y);
            let b1 = net_point(bottom_angle + strand_arc, net_bottom_radius, net_bottom_y);
            triangles.push(SourceTriangle { vertices: [t0, b0, b1], color: net_color, alpha: 176 });
            triangles.push(SourceTriangle { vertices: [t0, b1, t1], color: net_color, alpha: 176 });
        }
    }
    // Bottom loop of the net.
    for k in 0..NET_STRANDS {
        let a0 = std::f32::consts::TAU * (k as f32 / NET_STRANDS as f32);
        let a1 = std::f32::consts::TAU * ((k + 1) as f32 / NET_STRANDS as f32);
        let p0 = net_point(a0, net_bottom_radius, net_bottom_y);
        let p1 = net_point(a1, net_bottom_radius, net_bottom_y);
        let p2 = net_point(a1, net_bottom_radius * 0.98, net_bottom_y + 5.0);
        let p3 = net_point(a0, net_bottom_radius * 0.98, net_bottom_y + 5.0);
        triangles.push(SourceTriangle { vertices: [p0, p1, p2], color: net_color, alpha: 190 });
        triangles.push(SourceTriangle { vertices: [p0, p2, p3], color: net_color, alpha: 190 });
    }

    Mesh { triangles }
}

fn add_rect_outline_slabs(
    triangles: &mut Vec<SourceTriangle>,
    width: f32,
    height: f32,
    center: Vec3,
    thickness: f32,
    color: AppColor,
) {
    let slab_depth = 3.0;
    let horizontal = [
        (0.0, -(height - thickness) * 0.5, width, thickness),
        (0.0, (height - thickness) * 0.5, width, thickness),
    ];
    let vertical = [
        (-(width - thickness) * 0.5, 0.0, thickness, height - thickness * 2.0),
        ((width - thickness) * 0.5, 0.0, thickness, height - thickness * 2.0),
    ];
    for (dx, dy, w, h) in horizontal.into_iter().chain(vertical) {
        append_mesh_offset(
            triangles,
            rectangular_prism_mesh(w, h, slab_depth, color),
            Vec3::new(center.x + dx, center.y + dy, center.z),
        );
    }
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

fn snail_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut triangles = Vec::new();
    let body_color = scale_color(base_color, 0.92);
    let belly_color = scale_color(base_color, 0.72);
    let shell_color = AppColor::from_rgb(128, 84, 54);
    let shell_ridge = AppColor::from_rgb(222, 168, 92);
    let eye_color = AppColor::from_rgb(24, 28, 22);

    append_mesh_offset(
        &mut triangles,
        ellipsoid_mesh(size * 0.92, size * 0.32, size * 0.34, body_color),
        Vec3::new(-size * 0.04, size * 0.15, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.88, size * 0.08, size * 0.22, belly_color),
        Vec3::new(-size * 0.07, size * 0.32, -size * 0.01),
    );
    append_mesh_offset(
        &mut triangles,
        ellipsoid_mesh(size * 0.48, size * 0.48, size * 0.28, shell_color),
        Vec3::new(-size * 0.2, -size * 0.04, size * 0.04),
    );
    for radius_factor in [0.18, 0.29, 0.39] {
        add_disk(
            &mut triangles,
            Vec3::new(-size * 0.18, -size * 0.04, size * 0.2),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            size * radius_factor,
            shell_ridge,
            true,
        );
    }
    append_mesh_offset(
        &mut triangles,
        ellipsoid_mesh(size * 0.34, size * 0.28, size * 0.26, scale_color(base_color, 1.08)),
        Vec3::new(size * 0.4, size * 0.06, size * 0.02),
    );

    for eye_x in [size * 0.35, size * 0.52] {
        append_mesh_offset(
            &mut triangles,
            rectangular_prism_mesh(size * 0.035, size * 0.28, size * 0.035, body_color),
            Vec3::new(eye_x, -size * 0.17, size * 0.04),
        );
        append_mesh_offset(
            &mut triangles,
            ball_mesh(size * 0.08, eye_color),
            Vec3::new(eye_x, -size * 0.32, size * 0.07),
        );
    }

    Mesh { triangles }
}

fn fan_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut triangles = Vec::new();
    let shell = scale_color(base_color, 0.78);
    let bright = scale_color(base_color, 1.24);
    let dark = AppColor::from_rgb(22, 30, 38);
    let grille = AppColor::from_rgb(172, 238, 255);
    let blade = AppColor::from_rgb(242, 248, 252);

    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.56, size * 0.42, size * 0.38, shell),
        Vec3::new(-size * 0.05, 0.02 * size, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.28, size * 0.20, size * 0.34, bright),
        Vec3::new(size * 0.38, size * 0.02, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.34, size * 0.08, size * 0.22, dark),
        Vec3::new(size * 0.48, size * 0.02, size * 0.12),
    );

    add_disk(
        &mut triangles,
        Vec3::new(-size * 0.08, 0.0, size * 0.24),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.34,
        dark,
        true,
    );
    add_disk(
        &mut triangles,
        Vec3::new(-size * 0.08, 0.0, size * 0.27),
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        size * 0.29,
        scale_color(base_color, 0.48),
        true,
    );

    for angle in [0.0, 60.0, 120.0] {
        append_mesh_offset_rotated_z(
            &mut triangles,
            rectangular_prism_mesh(size * 0.42, size * 0.065, size * 0.045, blade),
            Vec3::new(-size * 0.08, 0.0, size * 0.31),
            angle,
        );
    }
    for offset in [-0.21, 0.0, 0.21] {
        append_mesh_offset(
            &mut triangles,
            rectangular_prism_mesh(size * 0.68, size * 0.022, size * 0.035, grille),
            Vec3::new(-size * 0.08, size * offset, size * 0.34),
        );
        append_mesh_offset(
            &mut triangles,
            rectangular_prism_mesh(size * 0.022, size * 0.68, size * 0.035, grille),
            Vec3::new(-size * 0.08 + size * offset, 0.0, size * 0.345),
        );
    }
    append_mesh_offset(
        &mut triangles,
        ball_mesh(size * 0.12, AppColor::from_rgb(255, 185, 84)),
        Vec3::new(-size * 0.08, 0.0, size * 0.36),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.22, size * 0.14, size * 0.12, AppColor::from_rgb(255, 176, 76)),
        Vec3::new(size * 0.66, size * 0.02, size * 0.10),
    );

    Mesh { triangles }
}

fn quad_drone_mesh(size: f32, base_color: AppColor) -> Mesh {
    let mut triangles = Vec::new();
    let body = scale_color(base_color, 0.92);
    let trim = AppColor::from_rgb(28, 34, 44);
    let arm = AppColor::from_rgb(84, 98, 116);
    let rotor_guard = AppColor::from_rgb(74, 230, 255);
    let rotor_blade = AppColor::from_rgb(226, 236, 244);
    let accent = AppColor::from_rgb(255, 176, 76);

    append_mesh_offset(
        &mut triangles,
        ellipsoid_mesh(size * 0.42, size * 0.26, size * 0.22, body),
        Vec3::new(0.0, 0.0, size * 0.02),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.94, size * 0.055, size * 0.08, arm),
        Vec3::new(0.0, 0.0, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.055, size * 0.62, size * 0.08, arm),
        Vec3::new(0.0, 0.0, 0.0),
    );
    append_mesh_offset(
        &mut triangles,
        rectangular_prism_mesh(size * 0.24, size * 0.06, size * 0.05, accent),
        Vec3::new(size * 0.12, 0.0, size * 0.15),
    );

    for (index, (x, y)) in [
        (-size * 0.43, -size * 0.30),
        (size * 0.43, -size * 0.30),
        (-size * 0.43, size * 0.30),
        (size * 0.43, size * 0.30),
    ]
    .into_iter()
    .enumerate()
    {
        append_mesh_offset(
            &mut triangles,
            ellipsoid_mesh(size * 0.18, size * 0.18, size * 0.08, trim),
            Vec3::new(x, y, size * 0.05),
        );
        add_disk(
            &mut triangles,
            Vec3::new(x, y, size * 0.18),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            size * 0.18,
            rotor_guard,
            true,
        );
        add_disk(
            &mut triangles,
            Vec3::new(x, y, size * 0.205),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            size * 0.11,
            trim,
            true,
        );
        let blade_angle = if index % 2 == 0 { 18.0 } else { -18.0 };
        append_mesh_offset_rotated_z(
            &mut triangles,
            rectangular_prism_mesh(size * 0.34, size * 0.035, size * 0.035, rotor_blade),
            Vec3::new(x, y, size * 0.24),
            blade_angle,
        );
        append_mesh_offset_rotated_z(
            &mut triangles,
            rectangular_prism_mesh(size * 0.035, size * 0.34, size * 0.035, rotor_blade),
            Vec3::new(x, y, size * 0.245),
            blade_angle,
        );
    }

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

fn append_mesh_offset_rotated_z(triangles: &mut Vec<SourceTriangle>, mesh: Mesh, offset: Vec3, angle_degrees: f32) {
    triangles.extend(mesh.triangles.into_iter().map(|mut triangle| {
        for vertex in &mut triangle.vertices {
            *vertex = rotate_z(*vertex, angle_degrees);
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

>>>>>>> Stashed changes
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
