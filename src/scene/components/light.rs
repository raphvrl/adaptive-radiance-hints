use glam::Vec3;

pub const POINT_SHADOW_FACE_COUNT: usize = 6;
pub const POINT_SHADOW_FACE_NAMES: [&str; POINT_SHADOW_FACE_COUNT] =
    ["+X", "-X", "+Y", "-Y", "+Z", "-Z"];
pub const SHADOW_RESOLUTION_OPTIONS: [u32; 5] = [128, 256, 512, 1024, 2048];
pub const DEFAULT_SHADOW_RESOLUTION: u32 = 256;
pub const DEFAULT_SHADOW_NORMAL_BIAS: f32 = 1.0;
pub const MAX_SHADOW_NORMAL_BIAS: f32 = 10.0;
pub const DEFAULT_POINT_SHADOW_BIAS: f32 = 0.1;
pub const DEFAULT_SPOT_SHADOW_BIAS: f32 = 0.03;
pub const MAX_SHADOW_BIAS: f32 = 10.0;

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub shadow_resolution: u32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_faces_enabled: [bool; POINT_SHADOW_FACE_COUNT],
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            range: 8.0,
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
            shadow_bias: DEFAULT_POINT_SHADOW_BIAS,
            shadow_normal_bias: DEFAULT_SHADOW_NORMAL_BIAS,
            shadow_faces_enabled: [true; POINT_SHADOW_FACE_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SpotLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub shadow_resolution: u32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
}

impl Default for SpotLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            range: 8.0,
            shadow_resolution: DEFAULT_SHADOW_RESOLUTION,
            shadow_bias: DEFAULT_SPOT_SHADOW_BIAS,
            shadow_normal_bias: DEFAULT_SHADOW_NORMAL_BIAS,
            inner_angle: std::f32::consts::FRAC_PI_8,
            outer_angle: std::f32::consts::FRAC_PI_6,
        }
    }
}
