use anyhow::{Result, ensure};
use glam::{Mat4, camera::rh::proj::directx};

pub struct Camera {
    pub fovy: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn projection(&self, aspect_ratio: f32) -> Result<Mat4> {
        ensure!(
            aspect_ratio.is_finite() && aspect_ratio > 0.0,
            "camera aspect ratio must be positive and finite"
        );
        ensure!(
            self.fovy.is_finite()
                && self.fovy > 0.0
                && self.fovy < std::f32::consts::PI
                && self.near.is_finite()
                && self.far.is_finite()
                && self.near > 0.0
                && self.far > self.near,
            "camera projection parameters are invalid"
        );

        Ok(directx::perspective(
            self.fovy,
            aspect_ratio,
            self.near,
            self.far,
        ))
    }
}
