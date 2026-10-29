use glam::{Mat4, Quat, Vec2, Vec3};
use hecs::Entity;

use crate::scene::{
    Scene,
    components::Transform,
    picking::{self, Ray},
};

const HANDLE_HIT_RADIUS_PIXELS: f32 = 11.0;
const RING_SEGMENTS: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TransformMode {
    #[default]
    Translate,
    Rotate,
    Scale,
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

#[derive(Clone, Copy, Debug)]
pub struct GizmoDraw {
    pub center: Vec3,
    pub size: f32,
    pub mode: TransformMode,
    pub active_axis: Option<Axis>,
}

#[derive(Default)]
pub struct EditorState {
    selected_entity: Option<Entity>,
    mode: TransformMode,
    drag: Option<ActiveDrag>,
}

#[derive(Clone, Copy)]
struct ActiveDrag {
    entity: Entity,
    axis: Axis,
    center: Vec3,
    start_translation: Vec3,
    start_rotation: Quat,
    start_scale: Vec3,
    operation: DragOperation,
}

#[derive(Clone, Copy)]
enum DragOperation {
    Translate {
        plane_normal: Vec3,
        start_point: Vec3,
    },
    Rotate {
        start_vector: Vec3,
    },
    Scale {
        screen_axis: Vec2,
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

    pub fn set_mode(&mut self, mode: TransformMode) {
        self.mode = mode;
        self.drag = None;
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
        drop(transform);

        Some(GizmoDraw {
            center,
            size: scene.gizmo_world_size(center, viewport_height)?,
            mode: self.mode,
            active_axis: self.drag.map(|drag| drag.axis),
        })
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
        let Some(axis_handle) =
            hit_test_gizmo(cursor, viewport, view_proj, center, size, self.mode)
        else {
            return false;
        };
        let Some(ray) = picking::ray_from_cursor(cursor, viewport, view_proj) else {
            return false;
        };

        let axis = axis_handle.vector();
        let operation = match self.mode {
            TransformMode::Translate => {
                let plane_normal = ray.direction() - axis * ray.direction().dot(axis);
                let Some(plane_normal) = plane_normal.try_normalize() else {
                    return false;
                };
                let Some(start_point) = intersect_ray_plane(ray, center, plane_normal) else {
                    return false;
                };
                DragOperation::Translate {
                    plane_normal,
                    start_point,
                }
            }
            TransformMode::Rotate => {
                let Some(start_point) = intersect_ray_plane(ray, center, axis) else {
                    return false;
                };
                let Some(start_vector) = (start_point - center).try_normalize() else {
                    return false;
                };
                DragOperation::Rotate { start_vector }
            }
            TransformMode::Scale => {
                let Some(origin_screen) = project_to_screen(center, view_proj, viewport) else {
                    return false;
                };
                let Some(end_screen) = project_to_screen(center + axis * size, view_proj, viewport)
                else {
                    return false;
                };
                let projected_axis = end_screen - origin_screen;
                let axis_pixels = projected_axis.length();
                let Some(screen_axis) = projected_axis.try_normalize() else {
                    return false;
                };
                DragOperation::Scale {
                    screen_axis,
                    axis_pixels,
                    start_cursor: cursor,
                }
            }
        };

        self.drag = Some(ActiveDrag {
            entity,
            axis: axis_handle,
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
        let axis = drag.axis.vector();

        match drag.operation {
            DragOperation::Translate {
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
            DragOperation::Rotate { start_vector } => {
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
            DragOperation::Scale {
                screen_axis,
                axis_pixels,
                start_cursor,
            } => {
                let distance = (cursor - start_cursor).dot(screen_axis);
                let factor = (1.0 + distance / axis_pixels).max(0.01);
                let mut scale = drag.start_scale;
                scale[drag.axis.index()] = (scale[drag.axis.index()] * factor).max(0.01);
                let Ok(mut transform) = scene.world.get::<&mut Transform>(drag.entity) else {
                    return;
                };
                transform.scale = scale;
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
) -> Option<Axis> {
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
            closest = Some(axis);
            closest_distance = distance;
        }
    }

    closest
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
