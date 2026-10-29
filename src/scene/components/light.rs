use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
}

impl Default for PointLight {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 1.0,
            range: 8.0,
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
