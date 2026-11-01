use serde::{Deserialize, Serialize};

use crate::scene::{
    components::{
        DEFAULT_POINT_SHADOW_BIAS, DEFAULT_SHADOW_NORMAL_BIAS, DEFAULT_SHADOW_RESOLUTION,
        DEFAULT_SPOT_SHADOW_BIAS, MAX_SHADOW_BIAS, MAX_SHADOW_NORMAL_BIAS, POINT_SHADOW_FACE_COUNT,
        SHADOW_RESOLUTION_OPTIONS,
    },
    materials::{DEFAULT_MATERIAL_METALLIC, DEFAULT_MATERIAL_ROUGHNESS, MaterialId},
};

pub const SCENE_FORMAT_VERSION: u32 = 6;
const MAX_LEGACY_SHADOW_DEPTH_BIAS: f32 = 0.002;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SceneFile {
    pub version: u32,
    pub materials: Vec<MaterialData>,
    pub entities: Vec<EntityData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EntityData {
    pub name: String,
    pub transform: TransformData,
    #[serde(default)]
    pub mesh: Option<MeshData>,
    #[serde(default)]
    pub material: Option<MaterialId>,
    #[serde(default)]
    pub camera: Option<CameraData>,
    #[serde(default)]
    pub controller: Option<ControllerData>,
    #[serde(default)]
    pub point_light: Option<PointLightData>,
    #[serde(default)]
    pub spot_light: Option<SpotLightData>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransformData {
    pub translation: [f32; 3],
    pub rotation_xyzw: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeshData {
    pub asset: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialData {
    pub id: MaterialId,
    #[serde(alias = "albedo")]
    pub base_color: [f32; 3],
    #[serde(default = "default_material_metallic")]
    pub metallic: f32,
    #[serde(default = "default_material_roughness")]
    pub roughness: f32,
    pub emission: [f32; 3],
}

fn default_material_metallic() -> f32 {
    DEFAULT_MATERIAL_METALLIC
}

fn default_material_roughness() -> f32 {
    DEFAULT_MATERIAL_ROUGHNESS
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CameraData {
    pub fovy_radians: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerData {
    pub move_speed: f32,
    pub look_sensitivity: f32,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PointLightData {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    #[serde(default = "default_shadow_resolution")]
    pub shadow_resolution: u32,
    #[serde(default = "default_point_shadow_bias")]
    pub shadow_bias: f32,
    #[serde(
        default,
        rename = "shadow_depth_bias",
        skip_serializing_if = "Option::is_none"
    )]
    pub legacy_shadow_depth_bias: Option<f32>,
    #[serde(default = "default_shadow_normal_bias")]
    pub shadow_normal_bias: f32,
    #[serde(default = "all_point_shadow_faces_enabled")]
    pub shadow_faces_enabled: [bool; POINT_SHADOW_FACE_COUNT],
}

fn default_shadow_resolution() -> u32 {
    DEFAULT_SHADOW_RESOLUTION
}

fn default_shadow_normal_bias() -> f32 {
    DEFAULT_SHADOW_NORMAL_BIAS
}

fn default_point_shadow_bias() -> f32 {
    DEFAULT_POINT_SHADOW_BIAS
}

fn all_point_shadow_faces_enabled() -> [bool; POINT_SHADOW_FACE_COUNT] {
    [true; POINT_SHADOW_FACE_COUNT]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpotLightData {
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    #[serde(default = "default_shadow_resolution")]
    pub shadow_resolution: u32,
    #[serde(default = "default_spot_shadow_bias")]
    pub shadow_bias: f32,
    #[serde(
        default,
        rename = "shadow_depth_bias",
        skip_serializing_if = "Option::is_none"
    )]
    pub legacy_shadow_depth_bias: Option<f32>,
    #[serde(default = "default_shadow_normal_bias")]
    pub shadow_normal_bias: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
}

impl PointLightData {
    pub fn resolved_shadow_bias(&self) -> f32 {
        resolve_shadow_bias(self.shadow_bias, self.legacy_shadow_depth_bias)
    }
}

impl SpotLightData {
    pub fn resolved_shadow_bias(&self) -> f32 {
        resolve_shadow_bias(self.shadow_bias, self.legacy_shadow_depth_bias)
    }
}

fn resolve_shadow_bias(shadow_bias: f32, legacy_depth_bias: Option<f32>) -> f32 {
    // Version 4 stored the final normalized depth offset, while version 5 stores Godot's bias parameter.
    legacy_depth_bias
        .map(|bias| bias * 100.0)
        .unwrap_or(shadow_bias)
}

fn default_spot_shadow_bias() -> f32 {
    DEFAULT_SPOT_SHADOW_BIAS
}

impl SceneFile {
    pub fn validate(&self) -> anyhow::Result<()> {
        use anyhow::{Context, ensure};

        ensure!(
            (2..=SCENE_FORMAT_VERSION).contains(&self.version),
            "unsupported scene format version {} (supported versions are 2 through {})",
            self.version,
            SCENE_FORMAT_VERSION
        );
        ensure!(!self.entities.is_empty(), "scene contains no entities");

        let mut material_ids = std::collections::HashSet::with_capacity(self.materials.len());
        for material in &self.materials {
            let id = material.id.as_str().trim();
            ensure!(!id.is_empty(), "material id must not be empty");
            ensure!(
                material_ids.insert(material.id.clone()),
                "duplicate material id '{id}'"
            );
            ensure!(
                material
                    .base_color
                    .into_iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(&value)),
                "material '{id}' has an invalid base color"
            );
            ensure!(
                material.metallic.is_finite() && (0.0..=1.0).contains(&material.metallic),
                "material '{id}' has an invalid metallic value"
            );
            ensure!(
                material.roughness.is_finite() && (0.0..=1.0).contains(&material.roughness),
                "material '{id}' has an invalid roughness value"
            );
            ensure!(
                material
                    .emission
                    .into_iter()
                    .all(|value| value.is_finite() && value >= 0.0),
                "material '{id}' has an invalid emission"
            );
        }

        let mut names = std::collections::HashSet::with_capacity(self.entities.len());
        let mut camera_count = 0;

        for entity in &self.entities {
            let name = entity.name.trim();
            ensure!(!name.is_empty(), "scene entity name must not be empty");
            ensure!(names.insert(name), "duplicate scene entity name '{name}'");

            let translation = entity.transform.translation;
            let rotation = entity.transform.rotation_xyzw;
            let scale = entity.transform.scale;
            let rotation_length_squared = rotation.iter().map(|value| value * value).sum::<f32>();
            let scale_determinant = scale.iter().product::<f32>();
            ensure!(
                translation.into_iter().all(f32::is_finite),
                "entity '{name}' has a non-finite translation"
            );
            ensure!(
                rotation.into_iter().all(f32::is_finite)
                    && rotation_length_squared.is_finite()
                    && rotation_length_squared > f32::EPSILON,
                "entity '{name}' has an invalid rotation"
            );
            ensure!(
                scale.into_iter().all(f32::is_finite)
                    && scale_determinant.is_finite()
                    && scale_determinant.abs() > f32::EPSILON,
                "entity '{name}' has an invalid scale"
            );

            if let Some(mesh) = &entity.mesh {
                ensure!(
                    !mesh.asset.trim().is_empty(),
                    "entity '{name}' has an empty mesh asset id"
                );
            }
            ensure!(
                entity.material.is_none() || entity.mesh.is_some(),
                "entity '{name}' has a material but no mesh"
            );
            ensure!(
                entity.mesh.is_none() || entity.material.is_some(),
                "mesh entity '{name}' must reference a material"
            );
            if let Some(material_id) = &entity.material {
                ensure!(
                    material_ids.contains(material_id),
                    "entity '{name}' references unknown material '{}'",
                    material_id.as_str()
                );
            }

            if let Some(camera) = &entity.camera {
                camera_count += 1;
                crate::scene::components::Camera {
                    fovy: camera.fovy_radians,
                    near: camera.near,
                    far: camera.far,
                }
                .projection(1.0)
                .with_context(|| format!("camera on entity '{name}' is invalid"))?;
            }

            if let Some(controller) = &entity.controller {
                ensure!(
                    entity.camera.is_some(),
                    "controller on '{name}' requires a camera"
                );
                ensure!(
                    controller.move_speed.is_finite() && controller.move_speed > 0.0,
                    "controller on '{name}' has an invalid move speed"
                );
                ensure!(
                    controller.look_sensitivity.is_finite() && controller.look_sensitivity > 0.0,
                    "controller on '{name}' has an invalid look sensitivity"
                );
                ensure!(
                    controller.yaw.is_finite()
                        && controller.pitch.is_finite()
                        && controller.pitch.abs() < std::f32::consts::FRAC_PI_2,
                    "controller on '{name}' has an invalid orientation"
                );
            }

            if let Some(light) = &entity.point_light {
                ensure!(
                    SHADOW_RESOLUTION_OPTIONS.contains(&light.shadow_resolution),
                    "point light on '{name}' has an unsupported shadow resolution {}",
                    light.shadow_resolution
                );
                if let Some(legacy_bias) = light.legacy_shadow_depth_bias {
                    ensure!(
                        legacy_bias.is_finite()
                            && (0.0..=MAX_LEGACY_SHADOW_DEPTH_BIAS).contains(&legacy_bias),
                        "point light on '{name}' has an invalid legacy shadow depth bias"
                    );
                }
                let shadow_bias = light.resolved_shadow_bias();
                ensure!(
                    shadow_bias.is_finite() && (0.0..=MAX_SHADOW_BIAS).contains(&shadow_bias),
                    "point light on '{name}' has an invalid shadow bias"
                );
                ensure!(
                    light.shadow_normal_bias.is_finite()
                        && (0.0..=MAX_SHADOW_NORMAL_BIAS).contains(&light.shadow_normal_bias),
                    "point light on '{name}' has an invalid shadow normal bias"
                );
                ensure!(
                    light
                        .color
                        .into_iter()
                        .all(|value| value.is_finite() && value >= 0.0),
                    "point light on '{name}' has an invalid color"
                );
                ensure!(
                    light.intensity.is_finite() && light.intensity >= 0.0,
                    "point light on '{name}' has an invalid intensity"
                );
                ensure!(
                    light.range.is_finite() && light.range > 0.0,
                    "point light on '{name}' has an invalid range"
                );
            }

            if let Some(light) = &entity.spot_light {
                ensure!(
                    SHADOW_RESOLUTION_OPTIONS.contains(&light.shadow_resolution),
                    "spot light on '{name}' has an unsupported shadow resolution {}",
                    light.shadow_resolution
                );
                if let Some(legacy_bias) = light.legacy_shadow_depth_bias {
                    ensure!(
                        legacy_bias.is_finite()
                            && (0.0..=MAX_LEGACY_SHADOW_DEPTH_BIAS).contains(&legacy_bias),
                        "spot light on '{name}' has an invalid legacy shadow depth bias"
                    );
                }
                let shadow_bias = light.resolved_shadow_bias();
                ensure!(
                    shadow_bias.is_finite() && (0.0..=MAX_SHADOW_BIAS).contains(&shadow_bias),
                    "spot light on '{name}' has an invalid shadow bias"
                );
                ensure!(
                    light.shadow_normal_bias.is_finite()
                        && (0.0..=MAX_SHADOW_NORMAL_BIAS).contains(&light.shadow_normal_bias),
                    "spot light on '{name}' has an invalid shadow normal bias"
                );
                ensure!(
                    light
                        .color
                        .into_iter()
                        .all(|value| value.is_finite() && value >= 0.0),
                    "spot light on '{name}' has an invalid color"
                );
                ensure!(
                    light.intensity.is_finite() && light.intensity >= 0.0,
                    "spot light on '{name}' has an invalid intensity"
                );
                ensure!(
                    light.range.is_finite() && light.range > 0.0,
                    "spot light on '{name}' has an invalid range"
                );
                ensure!(
                    light.inner_angle.is_finite()
                        && light.inner_angle >= 0.0
                        && light.outer_angle.is_finite()
                        && light.outer_angle > light.inner_angle
                        && light.outer_angle < std::f32::consts::FRAC_PI_2,
                    "spot light on '{name}' has invalid cone angles"
                );
            }
        }

        ensure!(camera_count == 1, "scene must contain exactly one camera");
        Ok(())
    }
}
