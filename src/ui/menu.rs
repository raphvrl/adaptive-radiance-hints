use crate::editor::TransformMode;

pub fn show(root_ui: &mut egui::Ui, current_mode: TransformMode) -> (bool, Option<TransformMode>) {
    let mut quit_requested = false;
    let mut mode = current_mode;

    egui::Panel::top("main_menu_bar")
        .frame(egui::Frame::side_top_panel(root_ui.style()))
        .show(root_ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        quit_requested = true;
                    }
                });

                ui.separator();
                ui.selectable_value(&mut mode, TransformMode::Translate, "Translate");
                ui.selectable_value(&mut mode, TransformMode::Rotate, "Rotate");
                ui.selectable_value(&mut mode, TransformMode::Scale, "Scale");

                ui.menu_button("Help", |ui| {
                    ui.label("WASD / arrows: move");
                    ui.label("Space / left Shift: move vertically");
                    ui.label("Right mouse + move: look");
                    ui.label("Left mouse: select object or drag gizmo");
                });
            });
        });

    let changed_mode = (mode != current_mode).then_some(mode);
    (quit_requested, changed_mode)
}
