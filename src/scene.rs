pub mod components;
pub mod picking;
pub mod systems;

use anyhow::{Context, Result, ensure};
use components::{Camera, Controller, Mesh, Transform};
use glam::Mat4;
use hecs::Entity;

use crate::{handle::MeshHandle, input::Input};

pub struct MeshInstance {
    pub mesh: MeshHandle,
    pub model: Mat4,
    pub selected: bool,
}

#[derive(Default)]
pub struct Scene {
    pub world: hecs::World,
}

impl Scene {
    pub fn new(cube_mesh: MeshHandle) -> Self {
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

        scene.world.spawn((
            Mesh { handle: cube_mesh },
            Transform::from_xyz(-0.75, 0.0, 0.0),
        ));

        scene.world.spawn((
            Mesh { handle: cube_mesh },
            Transform::from_xyz(0.75, 0.0, 0.0),
        ));

        scene
    }

    pub fn mesh_instances(&self, selected: Option<Entity>) -> Vec<MeshInstance> {
        let mut query = self.world.query::<(Entity, &Mesh, &Transform)>();

        query
            .iter()
            .map(|(entity, mesh, transform)| MeshInstance {
                mesh: mesh.handle,
                model: model_matrix(transform),
                selected: selected == Some(entity),
            })
            .collect()
    }

    pub fn pick_entity(&self, ray: picking::Ray) -> Option<Entity> {
        let mut query = self.world.query::<(Entity, &Mesh, &Transform)>();

        query
            .iter()
            .filter_map(|(entity, _, transform)| {
                picking::intersect_unit_cube(ray, model_matrix(transform))
                    .map(|distance| (entity, distance))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(entity, _)| entity)
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

fn model_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        transform.scale,
        transform.rotation,
        transform.translation,
    )
}
