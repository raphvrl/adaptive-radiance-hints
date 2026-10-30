use glam::Vec3;

use crate::scene::materials::MaterialId;

pub struct MaterialInspectorData {
    pub entity_name: String,
    pub material_id: MaterialId,
    pub albedo: [f32; 3],
    pub emission: [f32; 3],
    pub users: usize,
}

pub struct MaterialEdit {
    pub material_id: MaterialId,
    pub albedo: [f32; 3],
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
    pub shadow_resolution: Option<u32>,
    pub shadow_face_visibility: Option<[bool; 6]>,
    pub shadow_view_visibility: Option<bool>,
}

pub enum LightEdit {
    Point {
        color: [f32; 3],
        intensity: f32,
        range: f32,
    },
    Spot {
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    },
}

pub fn show(
    root_ui: &mut egui::Ui,
    material: Option<&MaterialInspectorData>,
) -> Option<MaterialEdit> {
    let material = material?;
    let mut edit = None;

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

            let mut albedo = material.albedo;
            let mut emission_intensity = material.emission.into_iter().fold(0.0_f32, f32::max);
            let mut emission_color = if emission_intensity > f32::EPSILON {
                material
                    .emission
                    .map(|channel| channel / emission_intensity)
            } else {
                [1.0; 3]
            };

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Albedo");
                if ui.color_edit_button_rgb(&mut albedo).changed() {
                    edit = Some(MaterialEdit {
                        material_id: material.material_id.clone(),
                        albedo,
                        emission: material.emission,
                    });
                }
            });

            ui.horizontal(|ui| {
                ui.label("Emission");
                if ui.color_edit_button_rgb(&mut emission_color).changed() {
                    edit = Some(MaterialEdit {
                        material_id: material.material_id.clone(),
                        albedo,
                        emission: (Vec3::from_array(emission_color) * emission_intensity)
                            .to_array(),
                    });
                }
            });

            let intensity_changed = ui
                .add(
                    egui::Slider::new(&mut emission_intensity, 0.0..=100.0)
                        .text("Emission intensity"),
                )
                .changed();
            if intensity_changed {
                edit = Some(MaterialEdit {
                    material_id: material.material_id.clone(),
                    albedo,
                    emission: (Vec3::from_array(emission_color) * emission_intensity).to_array(),
                });
            }
        });

    edit
}

pub fn show_light(root_ui: &mut egui::Ui, light: Option<&LightInspectorData>) -> Option<LightEdit> {
    let light = light?;

    let mut color = light.color;
    let mut intensity = light.intensity;
    let mut range = light.range;
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
            if let Some(resolution) = light.shadow_resolution {
                let map_kind = if light.is_spot { "map" } else { "per face" };
                ui.small(format!(
                    "Shadow resolution: {resolution} x {resolution} px ({map_kind})"
                ));
            } else {
                ui.small("Shadow resolution: pending");
            }

            if light.is_spot {
                let mut visible = light.shadow_view_visibility.unwrap_or(false);
                ui.add_enabled(
                    false,
                    egui::Checkbox::new(&mut visible, "Shadow map"),
                );
            } else {
                let faces = light.shadow_face_visibility.unwrap_or([false; 6]);
                egui::Grid::new("shadow_face_visibility")
                    .num_columns(3)
                    .show(ui, |ui| {
                        for (index, (face, mut visible)) in
                            ["+X", "-X", "+Y", "-Y", "+Z", "-Z"]
                                .into_iter()
                                .zip(faces)
                                .enumerate()
                        {
                            ui.add_enabled(false, egui::Checkbox::new(&mut visible, face));
                            if index % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
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
            inner_angle: inner_degrees.to_radians(),
            outer_angle: outer_degrees.to_radians(),
        })
    } else {
        Some(LightEdit::Point {
            color,
            intensity,
            range,
        })
    }
}
