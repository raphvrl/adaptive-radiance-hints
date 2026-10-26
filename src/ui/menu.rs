pub fn show(root_ui: &mut egui::Ui) -> bool {
    let mut quit_requested = false;

    egui::Panel::top("main_menu_bar")
        .frame(egui::Frame::side_top_panel(root_ui.style()))
        .show(root_ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        quit_requested = true;
                    }
                });

                ui.menu_button("Help", |ui| {
                    ui.label("WASD / arrows: move");
                    ui.label("Space / left Shift: move vertically");
                    ui.label("Right mouse + move: look");
                    ui.label("Left mouse: select block");
                });
            });
        });

    quit_requested
}
