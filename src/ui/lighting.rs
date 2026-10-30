use glam::Vec3;

use crate::renderer::AmbientLighting;

pub fn show_ambient(
    root_ui: &mut egui::Ui,
    ambient_lighting: AmbientLighting,
    show_window: &mut bool,
) -> Option<AmbientLighting> {
    let defaults = AmbientLighting::default();
    let mut color = ambient_lighting.color.to_array();
    let mut intensity = ambient_lighting.intensity;
    let mut changed = false;

    egui::Window::new("Ambient Light")
        .open(show_window)
        .default_pos(root_ui.max_rect().left_top() + egui::vec2(12.0, 12.0))
        .default_width(300.0)
        .show(root_ui.ctx(), |ui| {
            ui.heading("Environment lighting");
            ui.small("Diffuse fill applied uniformly across the scene.");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Color");
                changed |= ui.color_edit_button_rgb(&mut color).changed();
            });

            changed |= ui
                .add(egui::Slider::new(&mut intensity, 0.0..=2.0).text("Intensity"))
                .changed();

            ui.add_space(4.0);
            if ui.button("Reset to default").clicked() {
                color = defaults.color.to_array();
                intensity = defaults.intensity;
                changed = true;
            }
        });

    changed.then_some(AmbientLighting {
        color: Vec3::from_array(color),
        intensity,
    })
}
