use crate::editor::{TransformMode, ViewMode};

use super::AddLightKind;

pub fn show(
    root_ui: &mut egui::Ui,
    current_mode: TransformMode,
    current_view_mode: ViewMode,
) -> (
    bool,
    bool,
    Option<TransformMode>,
    Option<AddLightKind>,
    Option<ViewMode>,
) {
    let mut quit_requested = false;
    let mut save_requested = false;
    let mut add_light = None;
    let mut mode = current_mode;
    let mut view_mode = current_view_mode;

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
                ui.selectable_value(&mut mode, TransformMode::Translate, "Translate");
                ui.selectable_value(&mut mode, TransformMode::Rotate, "Rotate");
                ui.selectable_value(&mut mode, TransformMode::Scale, "Scale");

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
                    if ui.selectable_value(&mut view_mode, ViewMode::Scene, "Scene").clicked() {
                        ui.close();
                    }
                    if ui
                        .selectable_value(
                            &mut view_mode,
                            ViewMode::LightMarkers,
                            "Light Markers",
                        )
                        .clicked()
                    {
                        ui.close();
                    }
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

    let changed_mode = (mode != current_mode).then_some(mode);
    let changed_view_mode = (view_mode != current_view_mode).then_some(view_mode);

    (
        quit_requested,
        save_requested,
        changed_mode,
        add_light,
        changed_view_mode,
    )
}
