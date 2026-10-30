mod inspector;
mod lighting;
mod menu;
mod paint;

use std::time::Duration;

use crate::{editor::ViewMode, renderer::AmbientLighting};
use winit::{event::WindowEvent, window::Window};

pub use inspector::{LightEdit, LightInspectorData, MaterialEdit, MaterialInspectorData};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddLightKind {
    Point,
    Spot,
}

pub struct UiResponse {
    pub quit_requested: bool,
    pub save_requested: bool,
    pub add_light: Option<AddLightKind>,
    pub material_edit: Option<MaterialEdit>,
    pub light_edit: Option<LightEdit>,
    pub ambient_lighting_edit: Option<AmbientLighting>,
    pub repaint_delay: Duration,
    pub view_mode: Option<ViewMode>,
}

pub struct UiFrame {
    output: egui::FullOutput,
    pub response: UiResponse,
}

pub struct Ui {
    context: egui::Context,
    platform: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    show_ambient_window: bool,
}

impl Ui {
    pub fn new(window: &Window, device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let context = egui::Context::default();

        let platform = egui_winit::State::new(
            context.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let renderer =
            egui_wgpu::Renderer::new(device, target_format, egui_wgpu::RendererOptions::default());

        Self {
            context,
            platform,
            renderer,
            show_ambient_window: false,
        }
    }

    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
    ) -> egui_winit::EventResponse {
        self.platform.on_window_event(window, event)
    }

    pub fn wants_pointer_input(&self) -> bool {
        self.context.egui_wants_pointer_input()
    }

    pub fn run(
        &mut self,
        window: &Window,
        view_mode: ViewMode,
        material: Option<MaterialInspectorData>,
        light: Option<LightInspectorData>,
        ambient_lighting: AmbientLighting,
    ) -> UiFrame {
        let input = self.platform.take_egui_input(window);

        let (mut quit_requested, mut save_requested, mut add_light) = (false, false, None);
        let mut material_edit = None;
        let mut light_edit = None;
        let mut ambient_lighting_edit = None;
        let mut view_mode_change = None;
        let mut show_ambient_window = self.show_ambient_window;
        let mut output = self.context.run_ui(input, |root_ui| {
            let (quit, save, new_light, selected_view_mode) =
                menu::show(root_ui, view_mode, &mut show_ambient_window);
            quit_requested |= quit;
            save_requested |= save;
            add_light = new_light;
            view_mode_change = selected_view_mode;

            material_edit = inspector::show(root_ui, material.as_ref());
            light_edit = inspector::show_light(root_ui, light.as_ref());
            ambient_lighting_edit =
                lighting::show_ambient(root_ui, ambient_lighting, &mut show_ambient_window);
        });
        self.show_ambient_window = show_ambient_window;
        self.platform
            .handle_platform_output(window, std::mem::take(&mut output.platform_output));

        let repaint_delay = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);

        UiFrame {
            output,
            response: UiResponse {
                quit_requested,
                save_requested,
                add_light,
                material_edit,
                light_edit,
                ambient_lighting_edit,
                repaint_delay,
                view_mode: view_mode_change,
            },
        }
    }
}
