pub mod components;
pub mod format;
pub mod io;
pub mod materials;
pub mod picking;
pub mod systems;

use anyhow::{Context, Result, ensure};
use components::{Camera, MaterialRef, Mesh, PointLight, SpotLight, Transform};
use glam::{Mat4, Vec3};
use hecs::Entity;

use crate::{handle::MeshHandle, input::Input};
use materials::MaterialLibrary;

pub struct MeshInstance {
    pub mesh: MeshHandle,
    pub model: Mat4,
    pub albedo: Vec3,
    pub emission: Vec3,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct PointLightInstance {
    pub position: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SpotLightInstance {
    pub position: Vec3,
    pub direction: Vec3,
    pub color: Vec3,
    pub intensity: f32,
    pub range: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
}

#[derive(Default)]
pub struct Scene {
    pub world: hecs::World,
    pub materials: MaterialLibrary,
}

impl Scene {
    pub fn mesh_instances(&self, selected: Option<Entity>) -> Vec<MeshInstance> {
        let mut query = self
            .world
            .query::<(Entity, &Mesh, &Transform, Option<&MaterialRef>)>();

        query
            .iter()
            .map(|(entity, mesh, transform, material_ref)| {
                let material =
                    material_ref.and_then(|material_ref| self.materials.get(&material_ref.0));

                MeshInstance {
                    mesh: mesh.handle,
                    model: model_matrix(transform),
                    albedo: material.map_or(Vec3::ONE, |material| material.albedo),
                    emission: material.map_or(Vec3::ZERO, |material| material.emission),
                    selected: selected == Some(entity),
                }
            })
            .collect()
    }

    pub fn point_lights(&self) -> Vec<PointLightInstance> {
        let mut query = self.world.query::<(&PointLight, &Transform)>();

        query
            .iter()
            .map(|(light, transform)| PointLightInstance {
                position: transform.translation,
                color: light.color,
                intensity: light.intensity,
                range: light.range,
            })
            .collect()
    }

    pub fn spot_lights(&self) -> Vec<SpotLightInstance> {
        let mut query = self.world.query::<(&SpotLight, &Transform)>();

        query
            .iter()
            .map(|(light, transform)| SpotLightInstance {
                position: transform.translation,
                direction: (transform.rotation * Vec3::NEG_Z).normalize_or_zero(),
                color: light.color,
                intensity: light.intensity,
                range: light.range,
                inner_angle: light.inner_angle,
                outer_angle: light.outer_angle,
            })
            .collect()
    }

    pub fn pick_entity(&self, ray: picking::Ray) -> Option<Entity> {
        let mut query = self.world.query::<(
            Entity,
            &Transform,
            Option<&Mesh>,
            Option<&PointLight>,
            Option<&SpotLight>,
        )>();

        query
            .iter()
            .filter_map(|(entity, transform, mesh, point_light, spot_light)| {
                let mesh_hit =
                    mesh.and_then(|_| picking::intersect_unit_cube(ray, model_matrix(transform)));

                let light_hit = (point_light.is_some() || spot_light.is_some())
                    .then(|| picking::intersect_sphere(ray, transform.translation, 0.12))
                    .flatten();
                let distance = mesh_hit
                    .into_iter()
                    .chain(light_hit)
                    .min_by(f32::total_cmp)?;

                Some((entity, distance, light_hit.is_some()))
            })
            .min_by(|left, right| {
                right
                    .2
                    .cmp(&left.2)
                    .then_with(|| left.1.total_cmp(&right.1))
            })
            .map(|(entity, _, _)| entity)
    }

    pub fn gizmo_world_size(&self, position: Vec3, viewport_height: f32) -> Option<f32> {
        if !position.is_finite() || !viewport_height.is_finite() || viewport_height <= 0.0 {
            return None;
        }

        let mut query = self.world.query::<(&Camera, &Transform)>();
        let (camera, transform) = query.iter().next()?;
        let depth = (position - transform.translation).dot(transform.rotation * Vec3::NEG_Z);
        if !depth.is_finite() || depth <= 0.0 {
            return None;
        }

        let world_units_per_pixel = 2.0 * depth * (camera.fovy * 0.5).tan() / viewport_height;
        let size = world_units_per_pixel * 110.0;

        (size.is_finite() && size > 0.0).then_some(size)
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
