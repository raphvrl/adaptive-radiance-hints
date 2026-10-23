use std::sync::Arc;

use anyhow::Result;
use glam::Vec2;
use winit::{
    dpi::PhysicalSize,
    window::{Window, WindowId},
};

use crate::{
    clock::Clock,
    gpu::Gpu,
    input::{Input, InputButton},
    renderer::Renderer,
    scene::Scene,
};

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    input: Input,
    clock: Clock,
    scene: Scene,
    renderer: Renderer,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let gpu = Gpu::new(Arc::clone(&window)).await?;

        let input = Input::default();
        let clock = Clock::default();

        let scene = Scene::new();
        let renderer = Renderer::new(gpu.device(), gpu.surface_format())?;

        Ok(Self {
            window,
            gpu,
            input,
            clock,
            scene,
            renderer,
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn set_input_button(&mut self, button: InputButton, pressed: bool) {
        if pressed && !self.input.has_active_actions() {
            self.clock.reset();
        }

        self.input.set_button(button, pressed);
        self.request_redraw();
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.request_redraw();
    }

    pub fn add_mouse_delta(&mut self, delta: Vec2) {
        self.input.add_mouse_delta(delta);
        self.request_redraw();
    }

    pub fn update(&mut self) {
        let delta = self.clock.tick();
        self.scene.update(&self.input, delta);
        self.input.end_frame();

        if !self.input.has_active_actions() {
            self.clock.reset();
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.gpu.resize(size);
        self.request_redraw();
    }

    pub fn render(&mut self) -> Result<()> {
        let Some((frame, suboptimal)) = self.gpu.acquire_frame()? else {
            return Ok(());
        };

        let size = self.window.inner_size();
        let aspect_ratio = size.width as f32 / size.height as f32;
        let view_proj = self.scene.camera_view_projection(aspect_ratio)?;

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .render(self.gpu.device(), self.gpu.queue(), &view, view_proj)?;

        self.gpu.present(frame, suboptimal);

        if self.input.has_active_actions() {
            self.request_redraw();
        }

        Ok(())
    }
}
