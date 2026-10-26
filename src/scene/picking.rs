use glam::{Mat4, Vec2, Vec3, Vec4};

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    origin: Vec3,
    direction: Vec3,
}

pub fn ray_from_cursor(cursor_position: Vec2, viewport_size: Vec2, view_proj: Mat4) -> Option<Ray> {
    if !cursor_position.is_finite()
        || !viewport_size.is_finite()
        || viewport_size.x <= 0.0
        || viewport_size.y <= 0.0
    {
        return None;
    }

    let ndc = Vec2::new(
        cursor_position.x / viewport_size.x * 2.0 - 1.0,
        1.0 - cursor_position.y / viewport_size.y * 2.0,
    );
    let inverse_view_proj = view_proj.inverse();
    let near = unproject(inverse_view_proj, Vec3::new(ndc.x, ndc.y, 0.0))?;
    let far = unproject(inverse_view_proj, Vec3::new(ndc.x, ndc.y, 1.0))?;

    Some(Ray {
        origin: near,
        direction: (far - near).try_normalize()?,
    })
}

pub fn intersect_unit_cube(ray: Ray, model: Mat4) -> Option<f32> {
    let determinant = model.determinant();
    if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
        return None;
    }

    let inverse_model = model.inverse();
    let origin = inverse_model.transform_point3(ray.origin);
    let direction = inverse_model.transform_vector3(ray.direction);
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }

    let mut entry_distance = 0.0_f32;
    let mut exit_distance = f32::INFINITY;

    for axis in 0..3 {
        let origin = origin[axis];
        let direction = direction[axis];

        if direction.abs() <= f32::EPSILON {
            if !(-0.5..=0.5).contains(&origin) {
                return None;
            }
            continue;
        }

        let first = (-0.5 - origin) / direction;
        let second = (0.5 - origin) / direction;
        entry_distance = entry_distance.max(first.min(second));
        exit_distance = exit_distance.min(first.max(second));

        if entry_distance > exit_distance {
            return None;
        }
    }

    Some(entry_distance)
}

fn unproject(inverse_view_proj: Mat4, position: Vec3) -> Option<Vec3> {
    let world_position = inverse_view_proj * Vec4::new(position.x, position.y, position.z, 1.0);
    if !world_position.is_finite() || world_position.w.abs() <= f32::EPSILON {
        return None;
    }

    let world_position = world_position.truncate() / world_position.w;
    world_position.is_finite().then_some(world_position)
}
