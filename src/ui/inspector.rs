use glam::Vec3;

use crate::scene::{
    components::{
        MAX_SHADOW_BIAS, MAX_SHADOW_NORMAL_BIAS, POINT_SHADOW_FACE_COUNT, POINT_SHADOW_FACE_NAMES,
        SHADOW_RESOLUTION_OPTIONS,
    },
    materials::MaterialId,
};

pub struct MaterialInspectorData {
    pub entity_name: String,
    pub material_id: MaterialId,
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub emission: [f32; 3],
    pub users: usize,
}

pub struct MaterialEdit {
    pub material_id: MaterialId,
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    pub emission: [f32; 3],
}

pub struct LightInspectorData {
    pub entity_name: String,
    pub is_spot: bool,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub inner_angle: f32,
    pub outer_angle: f32,
    pub shadow_resolution: u32,
    pub shadow_bias: f32,
    pub shadow_normal_bias: f32,
    pub shadow_faces_enabled: Option<[bool; POINT_SHADOW_FACE_COUNT]>,
    pub shadow_face_camera_visibility: Option<[bool; POINT_SHADOW_FACE_COUNT]>,
    pub shadow_view_visibility: Option<bool>,
}

pub enum LightEdit {
    Point {
        color: [f32; 3],
        intensity: f32,
        range: f32,
        shadow_resolution: u32,
        shadow_bias: f32,
        shadow_normal_bias: f32,
        shadow_faces_enabled: [bool; POINT_SHADOW_FACE_COUNT],
    },
    Spot {
        color: [f32; 3],
        intensity: f32,
        range: f32,
        shadow_resolution: u32,
        shadow_bias: f32,
        shadow_normal_bias: f32,
        inner_angle: f32,
        outer_angle: f32,
    },
}

pub fn show(
    root_ui: &mut egui::Ui,
    material: Option<&MaterialInspectorData>,
) -> Option<MaterialEdit> {
    let material = material?;
    let mut changed = false;
    let mut base_color = material.base_color;
    let mut metallic = material.metallic;
    let mut roughness = material.roughness;
    let mut emission = material.emission;
    let mut emission_intensity = material.emission.into_iter().fold(0.0_f32, f32::max);
    let mut emission_color = if emission_intensity > f32::EPSILON {
        material
            .emission
            .map(|channel| channel / emission_intensity)
    } else {
        [1.0; 3]
    };

    egui::Window::new("Material")
        .default_pos(root_ui.max_rect().right_top() + egui::vec2(-300.0, 12.0))
        .default_width(280.0)
        .show(root_ui.ctx(), |ui| {
            ui.heading("Material");
            ui.separator();

            ui.label(format!("Object: {}", material.entity_name));
            ui.label(format!("Material: {}", material.material_id.as_str()));
            if material.users > 1 {
                ui.label(format!(
                    "Shared by {} objects; edits apply to all of them.",
                    material.users
                ));
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Base color");
                changed |= ui.color_edit_button_rgb(&mut base_color).changed();
            });

            changed |= ui
                .add(egui::Slider::new(&mut metallic, 0.0..=1.0).text("Metallic"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut roughness, 0.0..=1.0).text("Roughness"))
                .changed();

            ui.horizontal(|ui| {
                ui.label("Emission");
                if ui.color_edit_button_rgb(&mut emission_color).changed() {
                    emission = (Vec3::from_array(emission_color) * emission_intensity).to_array();
                    changed = true;
                }
            });

            let intensity_changed = ui
                .add(
                    egui::Slider::new(&mut emission_intensity, 0.0..=100.0)
                        .text("Emission intensity"),
                )
                .changed();
            if intensity_changed {
                emission = (Vec3::from_array(emission_color) * emission_intensity).to_array();
                changed = true;
            }
        });

    changed.then(|| MaterialEdit {
        material_id: material.material_id.clone(),
        base_color,
        metallic,
        roughness,
        emission,
    })
}

pub fn show_light(root_ui: &mut egui::Ui, light: Option<&LightInspectorData>) -> Option<LightEdit> {
    let light = light?;

    let mut color = light.color;
    let mut intensity = light.intensity;
    let mut range = light.range;
    let mut shadow_resolution = light.shadow_resolution;
    let mut shadow_bias = light.shadow_bias;
    let mut shadow_normal_bias = light.shadow_normal_bias;
    let mut shadow_faces_enabled = light
        .shadow_faces_enabled
        .unwrap_or([true; POINT_SHADOW_FACE_COUNT]);
    let mut inner_degrees = light.inner_angle.to_degrees();
    let mut outer_degrees = light.outer_angle.to_degrees();
    let mut changed = false;

    egui::Window::new("Light")
        .default_pos(root_ui.max_rect().right_top() + egui::vec2(-300.0, 250.0))
        .default_width(280.0)
        .show(root_ui.ctx(), |ui| {
            ui.heading(if light.is_spot {
                "Spot Light"
            } else {
                "Point Light"
            });
            ui.separator();
            ui.label(format!("Object: {}", light.entity_name));
            egui::ComboBox::from_label(if light.is_spot {
                "Shadow map resolution"
            } else {
                "Shadow face resolution"
            })
            .selected_text(format!("{shadow_resolution} x {shadow_resolution} px"))
            .show_ui(ui, |ui| {
                for resolution in SHADOW_RESOLUTION_OPTIONS {
                    changed |= ui
                        .selectable_value(
                            &mut shadow_resolution,
                            resolution,
                            format!("{resolution} x {resolution} px"),
                        )
                        .changed();
                }
            });
            changed |= ui
                .add(
                    egui::Slider::new(&mut shadow_bias, 0.0..=MAX_SHADOW_BIAS)
                        .text("Shadow bias")
                        .max_decimals(3),
                )
                .changed();
            changed |= ui
                .add(
                    egui::Slider::new(&mut shadow_normal_bias, 0.0..=MAX_SHADOW_NORMAL_BIAS)
                        .text("Shadow normal bias (texels)"),
                )
                .changed();

            if light.is_spot {
                let mut visible = light.shadow_view_visibility.unwrap_or(false);
                ui.add_enabled(false, egui::Checkbox::new(&mut visible, "Shadow map"));
            } else {
                ui.label("Shadow casting faces");
                egui::Grid::new("point_shadow_faces_enabled")
                    .num_columns(3)
                    .show(ui, |ui| {
                        for (index, face) in POINT_SHADOW_FACE_NAMES.into_iter().enumerate() {
                            changed |= ui
                                .checkbox(&mut shadow_faces_enabled[index], face)
                                .changed();
                            if index % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });

                if let Some(camera_visibility) = light.shadow_face_camera_visibility {
                    ui.label("Faces intersecting the camera frustum");
                    egui::Grid::new("point_shadow_face_camera_visibility")
                        .num_columns(3)
                        .show(ui, |ui| {
                            for (index, (face, mut visible)) in POINT_SHADOW_FACE_NAMES
                                .into_iter()
                                .zip(camera_visibility)
                                .enumerate()
                            {
                                ui.add_enabled(false, egui::Checkbox::new(&mut visible, face));
                                if index % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                }
            }

            ui.horizontal(|ui| {
                ui.label("Color");
                changed |= ui.color_edit_button_rgb(&mut color).changed();
            });
            changed |= ui
                .add(egui::Slider::new(&mut intensity, 0.0..=100.0).text("Intensity"))
                .changed();
            changed |= ui
                .add(egui::Slider::new(&mut range, 0.1..=30.0).text("Range"))
                .changed();
            ui.small("Gizmo brightness and marker size show intensity.");

            if light.is_spot {
                let max_inner = (outer_degrees - 1.0).max(0.0);
                changed |= ui
                    .add(
                        egui::Slider::new(&mut inner_degrees, 0.0..=max_inner)
                            .text("Inner angle (°)"),
                    )
                    .changed();
                let min_outer = (inner_degrees + 1.0).min(85.0);
                changed |= ui
                    .add(
                        egui::Slider::new(&mut outer_degrees, min_outer..=85.0)
                            .text("Outer angle (°)"),
                    )
                    .changed();
            }
        });

    if !changed {
        return None;
    }

    if light.is_spot {
        Some(LightEdit::Spot {
            color,
            intensity,
            range,
            shadow_resolution,
            shadow_bias,
            shadow_normal_bias,
            inner_angle: inner_degrees.to_radians(),
            outer_angle: outer_degrees.to_radians(),
        })
    } else {
        Some(LightEdit::Point {
            color,
            intensity,
            range,
            shadow_resolution,
            shadow_bias,
            shadow_normal_bias,
            shadow_faces_enabled,
        })
    }
}
