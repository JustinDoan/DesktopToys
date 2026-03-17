use std::{collections::HashMap, fs::File, io::BufReader, path::Path};

use anyhow::{bail, Context, Result};
use core_types::{AppColor, ObjectState, ObjectVisualKind, RectF, Vector2};
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
    imported_models: HashMap<String, Mesh>,
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
    depth: f32,
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

    pub fn build_vertices(&mut self, width: u32, height: u32, scene: &RenderScene<'_>) -> Result<Vec<GpuVertex>> {
        let mut triangles = Vec::new();
        for object in scene.objects {
            let mesh = self.mesh_for_object(object)?;
            let transform = compute_visual_transform(object, scene.bounds, scene.elapsed_seconds);
            for triangle in &mesh.triangles {
                triangles.push(transform_triangle(*triangle, transform, scene.elapsed_seconds));
            }
        }

        triangles.sort_by(|left, right| left.depth.partial_cmp(&right.depth).unwrap_or(std::cmp::Ordering::Equal));

        let mut vertices = Vec::with_capacity(triangles.len() * 3 + 4096);
        for triangle in triangles {
            emit_draw_triangle(&mut vertices, width, height, triangle);
        }

        emit_cursor(&mut vertices, width, height, scene.cursor);
        emit_panels(&mut vertices, width, height, scene.hud);
        Ok(vertices)
    }

    fn mesh_for_object(&mut self, object: &ObjectState) -> Result<Mesh> {
        let size = object.body.width.min(object.body.height).max(1.0);
        Ok(match object.visual_kind {
            ObjectVisualKind::Cube => cube_mesh(size, object.base_color),
            ObjectVisualKind::Dice => dice_mesh(size),
            ObjectVisualKind::Crystal => crystal_mesh(size, object.base_color),
            ObjectVisualKind::Satellite => satellite_mesh(size, object.base_color),
            ObjectVisualKind::ImportedModel => match object.model_source_path.as_deref() {
                Some(path) => self.imported_model_mesh(path, size, object.base_color)?,
                None => cube_mesh(size, object.base_color),
            },
        })
    }

    fn imported_model_mesh(&mut self, path: &str, target_size: f32, tint: AppColor) -> Result<Mesh> {
        let cache_key = format!("{path}|{target_size:.3}");
        if let Some(mesh) = self.imported_models.get(&cache_key) {
            return Ok(tint_mesh(mesh.clone(), tint));
        }

        let imported = load_mesh(path, target_size)?;
        self.imported_models.insert(cache_key, imported.clone());
        Ok(tint_mesh(imported, tint))
    }
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

fn emit_draw_triangle(vertices: &mut Vec<GpuVertex>, width: u32, height: u32, triangle: DrawTriangle) {
    let color = color_to_f32(triangle.color, triangle.alpha);
    for point in triangle.points {
        vertices.push(GpuVertex {
            position: to_ndc(point.x, point.y, depth_to_ndc(triangle.depth), width, height),
            color,
        });
    }
}

fn emit_rect(
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
    let left = x as f32;
    let top = y as f32;
    let right = (x + w) as f32;
    let bottom = (y + h) as f32;
    let p0 = Vec3::new(left, top, depth);
    let p1 = Vec3::new(right, top, depth);
    let p2 = Vec3::new(right, bottom, depth);
    let p3 = Vec3::new(left, bottom, depth);
    let alpha = color.a;
    let draw = DrawTriangle {
        points: [p0, p1, p2],
        color,
        depth,
        alpha,
    };
    emit_draw_triangle(vertices, width, height, draw);
    emit_draw_triangle(
        vertices,
        width,
        height,
        DrawTriangle {
            points: [p0, p2, p3],
            color,
            depth,
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

fn to_ndc(x: f32, y: f32, z: f32, width: u32, height: u32) -> [f32; 3] {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let ndc_x = (x / w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (y / h) * 2.0;
    [ndc_x, ndc_y, z]
}

fn depth_to_ndc(depth: f32) -> f32 {
    ((depth + 1024.0) / 2048.0).clamp(0.0, 1.0)
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
        ObjectVisualKind::ImportedModel => {
            center_z += 12.0;
        },
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
    let depth = (points[0].z + points[1].z + points[2].z) / 3.0;

    DrawTriangle {
        points,
        color: source.color,
        depth,
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
    let pip_half = size * 0.065;
    let pip_inset = size * 0.045;
    let pip_offset = size * 0.22;
    let pip_color = AppColor::from_rgb(42, 48, 58);

    add_z_face_pips(
        &mut mesh.triangles,
        hs - pip_inset,
        pip_half,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, pip_offset)],
    );
    add_z_face_pips(
        &mut mesh.triangles,
        -hs + pip_inset,
        pip_half,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (0.0, 0.0), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );
    add_x_face_pips(
        &mut mesh.triangles,
        hs - pip_inset,
        pip_half,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, 0.0), (-pip_offset, pip_offset)],
    );
    add_x_face_pips(
        &mut mesh.triangles,
        -hs + pip_inset,
        pip_half,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );
    add_y_face_pips(
        &mut mesh.triangles,
        -hs + pip_inset,
        pip_half,
        pip_color,
        &[(0.0, 0.0)],
    );
    add_y_face_pips(
        &mut mesh.triangles,
        hs - pip_inset,
        pip_half,
        pip_color,
        &[(-pip_offset, -pip_offset), (pip_offset, -pip_offset), (-pip_offset, 0.0), (pip_offset, 0.0), (-pip_offset, pip_offset), (pip_offset, pip_offset)],
    );

    mesh
}

fn add_z_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_z: f32,
    pip_half: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_x, center_y) in positions {
        let p0 = Vec3::new(center_x - pip_half, center_y - pip_half, pip_z);
        let p1 = Vec3::new(center_x + pip_half, center_y - pip_half, pip_z);
        let p2 = Vec3::new(center_x + pip_half, center_y + pip_half, pip_z);
        let p3 = Vec3::new(center_x - pip_half, center_y + pip_half, pip_z);
        extend_face_quad(triangles, [p0, p1, p2, p3], color, pip_z >= 0.0);
    }
}

fn add_x_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_x: f32,
    pip_half: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_z, center_y) in positions {
        let p0 = Vec3::new(pip_x, center_y - pip_half, center_z - pip_half);
        let p1 = Vec3::new(pip_x, center_y - pip_half, center_z + pip_half);
        let p2 = Vec3::new(pip_x, center_y + pip_half, center_z + pip_half);
        let p3 = Vec3::new(pip_x, center_y + pip_half, center_z - pip_half);
        extend_face_quad(triangles, [p0, p1, p2, p3], color, pip_x < 0.0);
    }
}

fn add_y_face_pips(
    triangles: &mut Vec<SourceTriangle>,
    pip_y: f32,
    pip_half: f32,
    color: AppColor,
    positions: &[(f32, f32)],
) {
    for &(center_x, center_z) in positions {
        let p0 = Vec3::new(center_x - pip_half, pip_y, center_z - pip_half);
        let p1 = Vec3::new(center_x + pip_half, pip_y, center_z - pip_half);
        let p2 = Vec3::new(center_x + pip_half, pip_y, center_z + pip_half);
        let p3 = Vec3::new(center_x - pip_half, pip_y, center_z + pip_half);
        extend_face_quad(triangles, [p0, p1, p2, p3], color, pip_y < 0.0);
    }
}

fn extend_face_quad(
    triangles: &mut Vec<SourceTriangle>,
    [p0, p1, p2, p3]: [Vec3; 4],
    color: AppColor,
    outward_winding: bool,
) {
    let (first, second) = if outward_winding {
        ([p0, p1, p2], [p0, p2, p3])
    } else {
        ([p0, p2, p1], [p0, p3, p2])
    };

    triangles.extend([
        SourceTriangle {
            vertices: first,
            color,
            alpha: 255,
        },
        SourceTriangle {
            vertices: second,
            color,
            alpha: 255,
        },
    ]);
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
