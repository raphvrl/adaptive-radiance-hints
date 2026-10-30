use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use glam::{Quat, Vec3};

use crate::{
    handle::MeshHandle,
    scene::{
        Scene,
        components::{
            Camera, Controller, MaterialRef, Mesh, Name, PointLight, SpotLight, Transform,
        },
        format::{
            CameraData, ControllerData, EntityData, MaterialData, MeshData, PointLightData,
            SCENE_FORMAT_VERSION, SceneFile, SpotLightData, TransformData,
        },
        materials::Material,
    },
};

pub fn load_scene(path: &Path, mesh_assets: &HashMap<String, MeshHandle>) -> Result<Scene> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("failed to read scene '{}'", path.display()))?;

    let scene_file: SceneFile = ron::from_str(&source)
        .with_context(|| format!("failed to parse scene '{}'", path.display()))?;

    scene_file.validate()?;

    let mut scene = Scene::default();
    for material in scene_file.materials {
        scene.materials.insert(
            material.id,
            Material {
                albedo: Vec3::from_array(material.albedo),
                emission: Vec3::from_array(material.emission),
            },
        );
    }

    for data in scene_file.entities {
        let transform = Transform {
            translation: Vec3::from_array(data.transform.translation),
            rotation: Quat::from_array(data.transform.rotation_xyzw).normalize(),
            scale: Vec3::from_array(data.transform.scale),
        };
        let entity = scene.world.spawn((Name(data.name), transform));

        if let Some(mesh) = data.mesh {
            let handle = mesh_assets
                .get(&mesh.asset)
                .copied()
                .with_context(|| format!("unknown mesh asset '{}'", mesh.asset))?;
            scene.world.insert_one(
                entity,
                Mesh {
                    handle,
                    asset_id: mesh.asset,
                },
            )?;
        }

        if let Some(material_id) = data.material {
            scene.world.insert_one(entity, MaterialRef(material_id))?;
        }

        if let Some(camera) = data.camera {
            scene.world.insert_one(
                entity,
                Camera {
                    fovy: camera.fovy_radians,
                    near: camera.near,
                    far: camera.far,
                },
            )?;
        }

        if let Some(controller) = data.controller {
            scene.world.insert_one(
                entity,
                Controller {
                    move_speed: controller.move_speed,
                    look_sensitivity: controller.look_sensitivity,
                    yaw: controller.yaw,
                    pitch: controller.pitch,
                },
            )?;
        }

        if let Some(light) = data.point_light {
            scene.world.insert_one(
                entity,
                PointLight {
                    color: Vec3::from_array(light.color),
                    intensity: light.intensity,
                    range: light.range,
                    shadow_faces_enabled: light.shadow_faces_enabled,
                },
            )?;
        }

        if let Some(light) = data.spot_light {
            scene.world.insert_one(
                entity,
                SpotLight {
                    color: Vec3::from_array(light.color),
                    intensity: light.intensity,
                    range: light.range,
                    inner_angle: light.inner_angle,
                    outer_angle: light.outer_angle,
                },
            )?;
        }
    }

    Ok(scene)
}

pub fn save_scene(path: &Path, scene: &Scene) -> Result<()> {
    let mut query = scene.world.query::<(
        &Name,
        &Transform,
        Option<&Mesh>,
        Option<&MaterialRef>,
        Option<&Camera>,
        Option<&Controller>,
        Option<&PointLight>,
        Option<&SpotLight>,
    )>();

    let mut entities = query
        .iter()
        .map(
            |(name, transform, mesh, material_ref, camera, controller, point_light, spot_light)| {
                EntityData {
                    name: name.0.clone(),
                    transform: TransformData {
                        translation: transform.translation.to_array(),
                        rotation_xyzw: transform.rotation.to_array(),
                        scale: transform.scale.to_array(),
                    },
                    mesh: mesh.map(|mesh| MeshData {
                        asset: mesh.asset_id.clone(),
                    }),
                    material: material_ref.map(|material_ref| material_ref.0.clone()),
                    camera: camera.map(|camera| CameraData {
                        fovy_radians: camera.fovy,
                        near: camera.near,
                        far: camera.far,
                    }),
                    controller: controller.map(|controller| ControllerData {
                        move_speed: controller.move_speed,
                        look_sensitivity: controller.look_sensitivity,
                        yaw: controller.yaw,
                        pitch: controller.pitch,
                    }),
                    point_light: point_light.map(|light| PointLightData {
                        color: light.color.to_array(),
                        intensity: light.intensity,
                        range: light.range,
                        shadow_faces_enabled: light.shadow_faces_enabled,
                    }),
                    spot_light: spot_light.map(|light| SpotLightData {
                        color: light.color.to_array(),
                        intensity: light.intensity,
                        range: light.range,
                        inner_angle: light.inner_angle,
                        outer_angle: light.outer_angle,
                    }),
                }
            },
        )
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| left.name.cmp(&right.name));

    let mut materials = scene
        .materials
        .iter()
        .map(|(id, material)| MaterialData {
            id: id.clone(),
            albedo: material.albedo.to_array(),
            emission: material.emission.to_array(),
        })
        .collect::<Vec<_>>();
    materials.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));

    let scene_file = SceneFile {
        version: SCENE_FORMAT_VERSION,
        materials,
        entities,
    };
    scene_file.validate()?;

    let content = ron::ser::to_string_pretty(&scene_file, ron::ser::PrettyConfig::default())
        .context("failed to serialize scene")?;
    fs::write(path, content).with_context(|| format!("failed to write scene '{}'", path.display()))
}
