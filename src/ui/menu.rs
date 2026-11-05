use super::AddLightKind;

pub fn show(
    root_ui: &mut egui::Ui,
    current_light_markers: bool,
    show_ambient_window: &mut bool,
    current_tile_heatmap: bool,
    current_voxel_debug: bool,
) -> (
    bool,
    bool,
    Option<AddLightKind>,
    Option<bool>,
    Option<bool>,
    Option<bool>,
) {
    let mut quit_requested = false;
    let mut save_requested = false;
    let mut add_light = None;
    let mut show_light_markers = current_light_markers;
    let mut show_tile_heatmap = current_tile_heatmap;
    let mut show_voxel_debug = current_voxel_debug;

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
                    ui.menu_button("Debug", |ui| {
                        ui.checkbox(&mut show_tile_heatmap, "Tile Light Heatmap");
                        ui.checkbox(&mut show_voxel_debug, "Voxel Occupancy");
                    });
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
                    ui.label("Tile heatmap colors show lights per tile; white means overflow");
                });
            });
        });

    let changed_light_markers =
        (show_light_markers != current_light_markers).then_some(show_light_markers);
    let changed_tile_heatmap =
        (show_tile_heatmap != current_tile_heatmap).then_some(show_tile_heatmap);
    let changed_voxel_debug = (show_voxel_debug != current_voxel_debug).then_some(show_voxel_debug);

    (
        quit_requested,
        save_requested,
        add_light,
        changed_light_markers,
        changed_tile_heatmap,
        changed_voxel_debug,
    )
}
