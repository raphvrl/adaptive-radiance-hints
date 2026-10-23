pub mod components;
pub mod systems;

use anyhow::{Context, Result, ensure};
use components::{Camera, Controller, Transform};
use glam::Mat4;

use crate::input::Input;

#[derive(Default)]
pub struct Scene {
    pub world: hecs::World,
}

impl Scene {
    pub fn new() -> Self {
        let mut scene = Self::default();

        scene.world.spawn((
            Camera {
                fovy: 60.0_f32.to_radians(),
                near: 0.1,
                far: 100.0,
            },
            Controller::default(),
            Transform::from_xyz(0.0, 0.0, 3.0),
        ));

        scene
    }

    pub fn update(&mut self, input: &Input, delta_time: f32) {
        systems::controller::update(&mut self.world, input, delta_time);
    }

    pub fn camera_view_projection(&self, aspect_ratio: f32) -> Result<Mat4> {
        let mut query = self.world.query::<(&Camera, &Transform)>();
        let mut cameras = query.iter();

        let (camera, transform) = cameras
            .next()
            .context("scene has no camera with a Transform")?;

        ensure!(cameras.next().is_none(), "scene has more than one camera");

        let camera_world =
            Mat4::from_rotation_translation(transform.rotation, transform.translation);

        let view = camera_world.inverse();
        let projection = camera.projection(aspect_ratio)?;

        Ok(projection * view)
    }
}
