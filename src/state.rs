use std::sync::Arc;

use anyhow::Result;
use glam::Vec2;
use hecs::Entity;
use winit::{
    dpi::PhysicalSize,
    event::{MouseButton, WindowEvent},
    window::{Window, WindowId},
};

use crate::{
    clock::Clock,
    gpu::Gpu,
    input::{Input, InputButton},
    renderer::Renderer,
    scene::{Scene, picking},
    ui::{Ui, UiResponse},
};

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    input: Input,
    clock: Clock,
    scene: Scene,
    selected_entity: Option<Entity>,
    renderer: Renderer,
    ui: Ui,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let gpu = Gpu::new(Arc::clone(&window)).await?;

        let input = Input::default();
        let clock = Clock::default();

        let size = window.inner_size();
        let mut renderer =
            Renderer::new(gpu.device(), gpu.surface_format(), size.width, size.height)?;

        let cube_mesh = renderer.register_mesh(gpu.device(), &crate::renderer::meshes::cube())?;

        let scene = Scene::new(cube_mesh);

        let ui = Ui::new(&window, gpu.device(), gpu.surface_format());

        Ok(Self {
            window,
            gpu,
            input,
            clock,
            scene,
            selected_entity: None,
            renderer,
            ui,
        })
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) -> egui_winit::EventResponse {
        let response = self.ui.on_window_event(&self.window, event);

        if let WindowEvent::CursorMoved { position, .. } = event {
            self.input
                .set_cursor_position(Vec2::new(position.x as f32, position.y as f32));
        }

        if response.repaint {
            self.request_redraw();
        }

        response
    }

    pub fn set_input_button(&mut self, button: InputButton, pressed: bool) {
        if pressed && !self.input.has_active_actions() {
            self.clock.reset();
        }

        self.input.set_button(button, pressed);
        if pressed && button == InputButton::Mouse(MouseButton::Left) {
            self.select_at_cursor();
        }
        self.request_redraw();
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.request_redraw();
    }

    pub fn add_mouse_delta(&mut self, delta: Vec2) {
        if !self.ui.wants_pointer_input() {
            self.input.add_mouse_delta(delta);
            self.request_redraw();
        }
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
        self.renderer
            .resize(self.gpu.device(), size.width, size.height);
        self.request_redraw();
    }

    pub fn render(&mut self) -> Result<Option<UiResponse>> {
        let Some((frame, suboptimal)) = self.gpu.acquire_frame()? else {
            return Ok(None);
        };

        let size = self.window.inner_size();
        let aspect_ratio = size.width as f32 / size.height as f32;
        let view_proj = self.scene.camera_view_projection(aspect_ratio)?;
        let renderables = self.scene.mesh_instances(self.selected_entity);

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.gpu
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Frame encoder"),
                });

        self.renderer.encode(
            self.gpu.device(),
            self.gpu.queue(),
            &mut encoder,
            &view,
            view_proj,
            &renderables,
        )?;

        let mut ui_frame = self.ui.run(&self.window);
        let callback_commands = self.ui.encode(
            self.gpu.device(),
            self.gpu.queue(),
            &mut encoder,
            &view,
            size,
            &mut ui_frame,
        );

        self.gpu.queue().submit(
            callback_commands
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.ui.free_textures(&mut ui_frame);
        self.gpu.present(frame, suboptimal);

        if self.input.has_active_actions() {
            self.request_redraw();
        }

        Ok(Some(ui_frame.response))
    }

    fn select_at_cursor(&mut self) {
        if self.ui.wants_pointer_input() {
            return;
        }

        self.selected_entity = self.entity_at_cursor();
    }

    fn entity_at_cursor(&self) -> Option<Entity> {
        let cursor_position = self.input.cursor_position()?;
        let size = self.window.inner_size();
        let viewport = Vec2::new(size.width as f32, size.height as f32);

        if viewport.x == 0.0 || viewport.y == 0.0 {
            return None;
        }

        let view_proj = self
            .scene
            .camera_view_projection(viewport.x / viewport.y)
            .ok()?;

        let ray = picking::ray_from_cursor(cursor_position, viewport, view_proj)?;
        self.scene.pick_entity(ray)
    }
}
