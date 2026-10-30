use glam::Vec3;

pub const POINT_SHADOW_FACE_COUNT: usize = 6;
pub const POINT_SHADOW_FACE_NAMES: [&str; POINT_SHADOW_FACE_COUNT] =
    ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    /// Face order: +X, -X, +Y, -Y, +Z, -Z.
    pub shadow_faces_enabled: [bool; POINT_SHADOW_FACE_COUNT],
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            range: 8.0,
            shadow_faces_enabled: [true; POINT_SHADOW_FACE_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpotLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            range: 8.0,
            inner_angle: std::f32::consts::FRAC_PI_8,
            outer_angle: std::f32::consts::FRAC_PI_6,
        }
    }
}
