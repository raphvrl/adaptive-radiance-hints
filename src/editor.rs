use glam::{Mat4, Quat, Vec2, Vec3};
use hecs::Entity;

use crate::scene::{
    Scene,
    components::{PointLight, SpotLight, Transform},
    picking::{self, Ray},
};

const HANDLE_HIT_RADIUS_PIXELS: f32 = 11.0;
const CENTER_HANDLE_HIT_RADIUS_PIXELS: f32 = 9.0;
const RING_SEGMENTS: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransformMode {
    #[default]
    Translate,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewMode {
    #[default]
    Scene,
    LightMarkers,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Self; 3] = [Self::X, Self::Y, Self::Z];

    pub fn vector(self) -> Vec3 {
        match self {
            Self::X => Vec3::X,
            Self::Y => Vec3::Y,
            Self::Z => Vec3::Z,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GizmoHandle {
    Axis(Axis),
    Center,
}

#[derive(Clone, Copy, Debug)]
pub struct GizmoDraw {
    pub center: Vec3,
    pub size: f32,
    pub mode: TransformMode,
    pub active_handle: Option<GizmoHandle>,
    pub light: Option<LightGizmo>,
}

#[derive(Clone, Copy, Debug)]
pub struct LightMarkerDraw {
    pub center: Vec3,
    pub size: f32,
    pub color: Vec3,
    pub intensity: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum LightGizmo {
    Point {
        range: f32,
        intensity: f32,
        color: Vec3,
    },
    Spot {
        direction: Vec3,
        range: f32,
        intensity: f32,
        color: Vec3,
        inner_angle: f32,
        outer_angle: f32,
    },
}

#[derive(Default)]
pub struct EditorState {
    selected_entity: Option<Entity>,
    mode: TransformMode,
    view_mode: ViewMode,
    drag: Option<ActiveDrag>,
}

#[derive(Clone, Copy)]
struct ActiveDrag {
    entity: Entity,
    handle: GizmoHandle,
    center: Vec3,
    start_translation: Vec3,
    start_rotation: Quat,
    start_scale: Vec3,
    operation: DragOperation,
}

#[derive(Clone, Copy)]
enum DragOperation {
    Translate {
        axis: Vec3,
        plane_normal: Vec3,
        start_point: Vec3,
    },
    Rotate {
        axis: Vec3,
        start_vector: Vec3,
    },
    ScaleAxis {
        axis: Axis,
        screen_axis: Vec2,
        axis_pixels: f32,
        start_cursor: Vec2,
    },
    ScaleUniform {
        axis_pixels: f32,
        start_cursor: Vec2,
    },
}

impl EditorState {
    pub fn selected_entity(&self) -> Option<Entity> {
        self.selected_entity
    }

    pub fn mode(&self) -> TransformMode {
        self.mode
    }

    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    pub fn set_mode(&mut self, mode: TransformMode) {
        self.mode = mode;
        self.drag = None;
    }

    pub fn set_view_mode(&mut self, mode: ViewMode) {
        self.view_mode = mode;
    }

    pub fn select(&mut self, entity: Option<Entity>) {
        self.selected_entity = entity;
        self.drag = None;
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    pub fn end_drag(&mut self) {
        self.drag = None;
    }

    pub fn gizmo_draw(&self, scene: &Scene, viewport_height: f32) -> Option<GizmoDraw> {
        let entity = self.selected_entity?;
        let transform = scene.world.get::<&Transform>(entity).ok()?;
        let center = transform.translation;
        let direction = transform.rotation * Vec3::NEG_Z;
        drop(transform);

        let light = scene
            .world
            .get::<&SpotLight>(entity)
            .ok()
            .map(|light| LightGizmo::Spot {
                direction,
                range: light.range,
                intensity: light.intensity,
                color: light.color,
                inner_angle: light.inner_angle,
                outer_angle: light.outer_angle,
            })
            .or_else(|| {
                scene
                    .world
                    .get::<&PointLight>(entity)
                    .ok()
                    .map(|light| LightGizmo::Point {
                        range: light.range,
                        intensity: light.intensity,
                        color: light.color,
                    })
            });

        Some(GizmoDraw {
            center,
            size: scene.gizmo_world_size(center, viewport_height)?,
            mode: self.mode,
            active_handle: self.drag.map(|drag| drag.handle),
            light,
        })
    }

    pub fn light_marker_draws(
        &self,
        scene: &Scene,
        viewport_height: f32,
    ) -> Vec<LightMarkerDraw> {
        let mut query = scene
            .world
            .query::<(Entity, &Transform, Option<&PointLight>, Option<&SpotLight>)>();

        query
            .iter()
            .filter_map(|(entity, transform, point_light, spot_light)| {
                if self.selected_entity == Some(entity) {
                    return None;
                }

                let (color, intensity) = spot_light
                    .map(|light| (light.color, light.intensity))
                    .or_else(|| point_light.map(|light| (light.color, light.intensity)))?;
                let size = scene.gizmo_world_size(transform.translation, viewport_height)?;

                Some(LightMarkerDraw {
                    center: transform.translation,
                    size,
                    color,
                    intensity,
                })
            })
            .collect()
    }

    pub fn begin_drag(
        &mut self,
        scene: &Scene,
        cursor: Vec2,
        viewport: Vec2,
        view_proj: Mat4,
    ) -> bool {
        let Some(entity) = self.selected_entity else {
            return false;
        };

        let Some(transform) = scene.world.get::<&Transform>(entity).ok() else {
            return false;
        };
        let center = transform.translation;
        let start_translation = transform.translation;
        let start_rotation = transform.rotation;
        let start_scale = transform.scale;
        drop(transform);

        let Some(size) = scene.gizmo_world_size(center, viewport.y) else {
            return false;
        };
        let Some(handle) = hit_test_gizmo(cursor, viewport, view_proj, center, size, self.mode)
        else {
            return false;
        };
        let Some(ray) = picking::ray_from_cursor(cursor, viewport, view_proj) else {
            return false;
        };

        let operation = match (self.mode, handle) {
            (TransformMode::Translate, GizmoHandle::Axis(axis_handle)) => {
                let axis = axis_handle.vector();
                let plane_normal = ray.direction() - axis * ray.direction().dot(axis);
                let Some(plane_normal) = plane_normal.try_normalize() else {
                    return false;
                };
                let Some(start_point) = intersect_ray_plane(ray, center, plane_normal) else {
                    return false;
                };
                DragOperation::Translate {
                    axis,
                    plane_normal,
                    start_point,
                }
            }
            (TransformMode::Rotate, GizmoHandle::Axis(axis_handle)) => {
                let axis = axis_handle.vector();
                let Some(start_point) = intersect_ray_plane(ray, center, axis) else {
                    return false;
                };
                let Some(start_vector) = (start_point - center).try_normalize() else {
                    return false;
                };
                DragOperation::Rotate { axis, start_vector }
            }
            (TransformMode::Scale, GizmoHandle::Axis(axis)) => {
                let axis_vector = axis.vector();
                let Some(origin_screen) = project_to_screen(center, view_proj, viewport) else {
                    return false;
                };
                let Some(end_screen) =
                    project_to_screen(center + axis_vector * size, view_proj, viewport)
                else {
                    return false;
                };
                let projected_axis = end_screen - origin_screen;
                let axis_pixels = projected_axis.length();
                let Some(screen_axis) = projected_axis.try_normalize() else {
                    return false;
                };
                DragOperation::ScaleAxis {
                    axis,
                    screen_axis,
                    axis_pixels,
                    start_cursor: cursor,
                }
            }
            (TransformMode::Scale, GizmoHandle::Center) => {
                let Some(axis_pixels) = projected_gizmo_size(center, size, view_proj, viewport)
                else {
                    return false;
                };
                DragOperation::ScaleUniform {
                    axis_pixels,
                    start_cursor: cursor,
                }
            }
            _ => return false,
        };

        self.drag = Some(ActiveDrag {
            entity,
            handle,
            center,
            start_translation,
            start_rotation,
            start_scale,
            operation,
        });
        true
    }

    pub fn update_drag(&self, scene: &mut Scene, cursor: Vec2, viewport: Vec2, view_proj: Mat4) {
        let Some(drag) = self.drag else {
            return;
        };

        match drag.operation {
            DragOperation::Translate {
                axis,
                plane_normal,
                start_point,
            } => {
                let Some(ray) = picking::ray_from_cursor(cursor, viewport, view_proj) else {
                    return;
                };
                let Some(point) = intersect_ray_plane(ray, drag.center, plane_normal) else {
                    return;
                };
                let distance = (point - start_point).dot(axis);
                let Ok(mut transform) = scene.world.get::<&mut Transform>(drag.entity) else {
                    return;
                };
                transform.translation = drag.start_translation + axis * distance;
            }
            DragOperation::Rotate { axis, start_vector } => {
                let Some(ray) = picking::ray_from_cursor(cursor, viewport, view_proj) else {
                    return;
                };
                let Some(point) = intersect_ray_plane(ray, drag.center, axis) else {
                    return;
                };
                let Some(current_vector) = (point - drag.center).try_normalize() else {
                    return;
                };
                let angle = axis
                    .dot(start_vector.cross(current_vector))
                    .atan2(start_vector.dot(current_vector));
                let Ok(mut transform) = scene.world.get::<&mut Transform>(drag.entity) else {
                    return;
                };
                transform.rotation = Quat::from_axis_angle(axis, angle) * drag.start_rotation;
            }
            DragOperation::ScaleAxis {
                axis,
                screen_axis,
                axis_pixels,
                start_cursor,
            } => {
                let distance = (cursor - start_cursor).dot(screen_axis);
                let factor = (1.0 + distance / axis_pixels).max(0.01);
                let mut scale = drag.start_scale;
                scale[axis.index()] = (scale[axis.index()] * factor).max(0.01);
                let Ok(mut transform) = scene.world.get::<&mut Transform>(drag.entity) else {
                    return;
                };
                transform.scale = scale;
            }
            DragOperation::ScaleUniform {
                axis_pixels,
                start_cursor,
            } => {
                let distance = start_cursor.y - cursor.y;
                let min_scale = drag.start_scale.min_element().max(0.01);
                let min_factor = 0.01 / min_scale;
                let factor = (1.0 + distance / axis_pixels).max(min_factor);
                let Ok(mut transform) = scene.world.get::<&mut Transform>(drag.entity) else {
                    return;
                };
                transform.scale = drag.start_scale * factor;
            }
        }
    }
}

fn hit_test_gizmo(
    cursor: Vec2,
    viewport: Vec2,
    view_proj: Mat4,
    center: Vec3,
    size: f32,
    mode: TransformMode,
) -> Option<GizmoHandle> {
    if mode == TransformMode::Scale {
        let center_screen = project_to_screen(center, view_proj, viewport)?;
        if cursor.distance(center_screen) <= CENTER_HANDLE_HIT_RADIUS_PIXELS {
            return Some(GizmoHandle::Center);
        }
    }

    let mut closest = None;
    let mut closest_distance = HANDLE_HIT_RADIUS_PIXELS;

    for axis in Axis::ALL {
        let distance = match mode {
            TransformMode::Translate => axis_segment_distance(
                cursor,
                viewport,
                view_proj,
                center + axis.vector() * (size * 0.08),
                center + axis.vector() * size,
            ),
            TransformMode::Scale => axis_segment_distance(
                cursor,
                viewport,
                view_proj,
                center + axis.vector() * (size * 0.08),
                center + axis.vector() * (size * 0.95),
            ),
            TransformMode::Rotate => ring_distance(
                cursor,
                viewport,
                view_proj,
                center,
                axis.vector(),
                size * 0.84,
            ),
        };

        if distance < closest_distance {
            closest = Some(GizmoHandle::Axis(axis));
            closest_distance = distance;
        }
    }

    closest
}

fn projected_gizmo_size(center: Vec3, size: f32, view_proj: Mat4, viewport: Vec2) -> Option<f32> {
    let center_screen = project_to_screen(center, view_proj, viewport)?;

    Axis::ALL
        .into_iter()
        .filter_map(|axis| {
            project_to_screen(center + axis.vector() * size, view_proj, viewport)
                .map(|end| end.distance(center_screen))
        })
        .filter(|pixels| pixels.is_finite() && *pixels > f32::EPSILON)
        .max_by(f32::total_cmp)
}

fn axis_segment_distance(
    cursor: Vec2,
    viewport: Vec2,
    view_proj: Mat4,
    start: Vec3,
    end: Vec3,
) -> f32 {
    let (Some(start), Some(end)) = (
        project_to_screen(start, view_proj, viewport),
        project_to_screen(end, view_proj, viewport),
    ) else {
        return f32::INFINITY;
    };
    distance_to_segment(cursor, start, end)
}

fn ring_distance(
    cursor: Vec2,
    viewport: Vec2,
    view_proj: Mat4,
    center: Vec3,
    axis: Vec3,
    radius: f32,
) -> f32 {
    let (u, v) = perpendicular_basis(axis);
    let mut closest_distance = f32::INFINITY;

    for segment in 0..RING_SEGMENTS {
        let angle_a = std::f32::consts::TAU * segment as f32 / RING_SEGMENTS as f32;
        let angle_b = std::f32::consts::TAU * (segment + 1) as f32 / RING_SEGMENTS as f32;
        let point_a = center + (u * angle_a.cos() + v * angle_a.sin()) * radius;
        let point_b = center + (u * angle_b.cos() + v * angle_b.sin()) * radius;
        let (Some(point_a), Some(point_b)) = (
            project_to_screen(point_a, view_proj, viewport),
            project_to_screen(point_b, view_proj, viewport),
        ) else {
            continue;
        };
        closest_distance = closest_distance.min(distance_to_segment(cursor, point_a, point_b));
    }

    closest_distance
}

fn project_to_screen(point: Vec3, view_proj: Mat4, viewport: Vec2) -> Option<Vec2> {
    if !viewport.is_finite() || viewport.x <= 0.0 || viewport.y <= 0.0 {
        return None;
    }

    let clip = view_proj * point.extend(1.0);
    if !clip.is_finite() || clip.w <= f32::EPSILON {
        return None;
    }

    let ndc = clip.truncate() / clip.w;
    let screen = Vec2::new(
        (ndc.x + 1.0) * 0.5 * viewport.x,
        (1.0 - ndc.y) * 0.5 * viewport.y,
    );
    screen.is_finite().then_some(screen)
}

fn distance_to_segment(point: Vec2, start: Vec2, end: Vec2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }

    let amount = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * amount)
}

fn intersect_ray_plane(ray: Ray, point: Vec3, normal: Vec3) -> Option<Vec3> {
    let denominator = normal.dot(ray.direction());
    if denominator.abs() <= 1.0e-5 {
        return None;
    }

    let distance = normal.dot(point - ray.origin()) / denominator;
    if !distance.is_finite() || distance < 0.0 {
        return None;
    }

    let intersection = ray.origin() + ray.direction() * distance;
    intersection.is_finite().then_some(intersection)
}

fn perpendicular_basis(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u).normalize();
    (u, v)
}
