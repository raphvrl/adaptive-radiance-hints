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
