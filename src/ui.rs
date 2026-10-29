mod menu;
mod paint;

use std::time::Duration;

use crate::editor::TransformMode;
use winit::{event::WindowEvent, window::Window};

pub struct UiResponse {
    pub quit_requested: bool,
    pub repaint_delay: Duration,
    pub transform_mode: Option<TransformMode>,
}

pub struct UiFrame {
    output: egui::FullOutput,
    pub response: UiResponse,
}

pub struct Ui {
    context: egui::Context,
    platform: egui_winit::State,
    renderer: egui_wgpu::Renderer,
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

    pub fn run(&mut self, window: &Window, mode: TransformMode) -> UiFrame {
        let input = self.platform.take_egui_input(window);

        let (mut quit_requested, mut transform_mode) = (false, None);
        let mut output = self.context.run_ui(input, |root_ui| {
            let (quit, selected_mode) = menu::show(root_ui, mode);
            quit_requested |= quit;
            transform_mode = selected_mode;
        });
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
                repaint_delay,
                transform_mode,
            },
        }
    }
}
