use crate::editor::ViewMode;

use super::AddLightKind;

pub fn show(
    root_ui: &mut egui::Ui,
    current_view_mode: ViewMode,
    show_ambient_window: &mut bool,
) -> (bool, bool, Option<AddLightKind>, Option<ViewMode>) {
    let mut quit_requested = false;
    let mut save_requested = false;
    let mut add_light = None;
    let mut show_light_markers = current_view_mode == ViewMode::LightMarkers;

    egui::Panel::top("main_menu_bar")
        .frame(egui::Frame::side_top_panel(root_ui.style()))
        .show(root_ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Save Scene").clicked() {
                        save_requested = true;
                        ui.close();
                    }
                    if ui.button("Quit").clicked() {
                        quit_requested = true;
                    }
                });

                ui.separator();

                ui.menu_button("Create", |ui| {
                    if ui.button("Point Light").clicked() {
                        add_light = Some(AddLightKind::Point);
                        ui.close();
                    }
                    if ui.button("Spot Light").clicked() {
                        add_light = Some(AddLightKind::Spot);
                        ui.close();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut show_light_markers, "Light Markers");
                    ui.separator();
                    ui.checkbox(show_ambient_window, "Ambient Lighting");
                });

                ui.menu_button("Help", |ui| {
                    ui.label("1 / 2 / 3: Translate / Scale / Rotate");
                    ui.label("WASD / arrows: move");
                    ui.label("Space / left Shift: move vertically");
                    ui.label("Right mouse + move: look");
                    ui.label("Left mouse: select object or drag gizmo");
                    ui.label("Delete: remove selected object");
                });
            });
        });

    let view_mode = if show_light_markers {
        ViewMode::LightMarkers
    } else {
        ViewMode::Scene
    };
    let changed_view_mode = (view_mode != current_view_mode).then_some(view_mode);

    (quit_requested, save_requested, add_light, changed_view_mode)
}
