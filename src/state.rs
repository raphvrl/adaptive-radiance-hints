use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::Result;
use glam::{Mat4, Vec2};
use winit::{
    dpi::PhysicalSize,
    event::{MouseButton, WindowEvent},
    keyboard::KeyCode,
    window::{Window, WindowId},
};

use crate::{
    clock::Clock,
    editor::{EditorState, TransformMode},
    gpu::Gpu,
    input::{Input, InputButton},
    renderer::Renderer,
    scene::{
        Scene,
        components::{MaterialRef, Name},
        picking,
    },
    ui::{MaterialEdit, MaterialInspectorData, Ui, UiResponse},
};

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    input: Input,
    clock: Clock,
    scene: Scene,
    editor: EditorState,
    renderer: Renderer,
    ui: Ui,
    scene_path: PathBuf,
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

        let mesh_assets = HashMap::from([("builtin:cube".to_owned(), cube_mesh)]);
        let scene_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("scenes")
            .join("cornell.ron");
        let scene = crate::scene::io::load_scene(&scene_path, &mesh_assets)?;

        let ui = Ui::new(&window, gpu.device(), gpu.surface_format());

        Ok(Self {
            window,
            gpu,
            input,
            clock,
            scene,
            editor: EditorState::default(),
            renderer,
            ui,
            scene_path,
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
            let cursor = Vec2::new(position.x as f32, position.y as f32);
            self.input.set_cursor_position(cursor);
            if self.editor.is_dragging() {
                self.update_editor_drag(cursor);
                self.request_redraw();
            }
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
        match (button, pressed) {
            (InputButton::Key(KeyCode::Digit1), true) => {
                self.editor.set_mode(TransformMode::Translate)
            }
            (InputButton::Key(KeyCode::Digit2), true) => self.editor.set_mode(TransformMode::Scale),
            (InputButton::Key(KeyCode::Digit3), true) => {
                self.editor.set_mode(TransformMode::Rotate)
            }
            (InputButton::Key(KeyCode::Escape), true) => self.editor.select(None),
            (InputButton::Mouse(MouseButton::Left), true) => self.select_or_begin_drag(),
            (InputButton::Mouse(MouseButton::Left), false) => self.editor.end_drag(),
            _ => {}
        }
        self.request_redraw();
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.editor.end_drag();
        self.request_redraw();
    }

    pub fn add_mouse_delta(&mut self, delta: Vec2) {
        if !self.editor.is_dragging() && !self.ui.wants_pointer_input() {
            self.input.add_mouse_delta(delta);
            self.request_redraw();
        }
    }

    pub fn update(&mut self) {
        let delta = self.clock.tick();
        if !self.editor.is_dragging() {
            self.scene.update(&self.input, delta);
        }
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
        let material = self.selected_material_inspector();
        let mut ui_frame = self.ui.run(&self.window, self.editor.mode(), material);
        if let Some(mode) = ui_frame.response.transform_mode {
            self.editor.set_mode(mode);
        }
        if let Some(edit) = ui_frame.response.material_edit.take() {
            self.apply_material_edit(edit);
        }
        if ui_frame.response.save_requested {
            match crate::scene::io::save_scene(&self.scene_path, &self.scene) {
                Ok(()) => tracing::info!(path = %self.scene_path.display(), "scene saved"),
                Err(error) => tracing::error!(
                    error = %format!("{error:#}"),
                    path = %self.scene_path.display(),
                    "failed to save scene"
                ),
            }
        }
        let renderables = self.scene.mesh_instances(self.editor.selected_entity());
        let gizmo_draw = self.editor.gizmo_draw(&self.scene, size.height as f32);

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
            gizmo_draw,
        )?;

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

    fn select_or_begin_drag(&mut self) {
        if self.ui.wants_pointer_input() {
            return;
        }

        if let (Some(cursor), Some((viewport, view_proj))) = (
            self.input.cursor_position(),
            self.viewport_and_view_projection(),
        ) && self
            .editor
            .begin_drag(&self.scene, cursor, viewport, view_proj)
        {
            return;
        }

        self.editor.select(self.entity_at_cursor());
    }

    fn entity_at_cursor(&self) -> Option<hecs::Entity> {
        let cursor_position = self.input.cursor_position()?;
        let (viewport, view_proj) = self.viewport_and_view_projection()?;

        let ray = picking::ray_from_cursor(cursor_position, viewport, view_proj)?;
        self.scene.pick_entity(ray)
    }

    fn update_editor_drag(&mut self, cursor: Vec2) {
        let Some((viewport, view_proj)) = self.viewport_and_view_projection() else {
            return;
        };

        self.editor
            .update_drag(&mut self.scene, cursor, viewport, view_proj);
    }

    fn selected_material_inspector(&self) -> Option<MaterialInspectorData> {
        let entity = self.editor.selected_entity()?;
        let entity_name = self.scene.world.get::<&Name>(entity).ok()?.0.clone();
        let material_id = self.scene.world.get::<&MaterialRef>(entity).ok()?.0.clone();
        let material = *self.scene.materials.get(&material_id)?;

        let mut query = self.scene.world.query::<&MaterialRef>();
        let users = query
            .iter()
            .filter(|material_ref| material_ref.0 == material_id)
            .count();

        Some(MaterialInspectorData {
            entity_name,
            material_id,
            albedo: material.albedo.to_array(),
            emission: material.emission.to_array(),
            users,
        })
    }

    fn apply_material_edit(&mut self, edit: MaterialEdit) {
        let Some(material) = self.scene.materials.get_mut(&edit.material_id) else {
            return;
        };

        material.albedo = glam::Vec3::from_array(edit.albedo);
        material.emission = glam::Vec3::from_array(edit.emission);
    }

    fn viewport_and_view_projection(&self) -> Option<(Vec2, Mat4)> {
        let size = self.window.inner_size();
        let viewport = Vec2::new(size.width as f32, size.height as f32);
        if viewport.x <= 0.0 || viewport.y <= 0.0 {
            return None;
        }

        let view_proj = self
            .scene
            .camera_view_projection(viewport.x / viewport.y)
            .ok()?;
        Some((viewport, view_proj))
    }
}
