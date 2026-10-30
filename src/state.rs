use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Result;
use glam::{Mat4, Quat, Vec2, Vec3};
use winit::{
    dpi::PhysicalSize,
    event::{MouseButton, WindowEvent},
    keyboard::KeyCode,
    window::{Window, WindowId},
};

use crate::{
    clock::Clock,
    editor::{EditorState, TransformMode, ViewMode},
    gpu::Gpu,
    input::{Input, InputButton},
    renderer::{AmbientLighting, Renderer},
    scene::{
        Scene,
        components::{Camera, MaterialRef, Name, PointLight, SpotLight, Transform},
        picking,
    },
    ui::{
        AddLightKind, LightEdit, LightInspectorData, MaterialEdit, MaterialInspectorData, Ui,
        UiResponse,
    },
};

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    input: Input,
    clock: Clock,
    scene: Scene,
    editor: EditorState,
    renderer: Renderer,
    ambient_lighting: AmbientLighting,
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
            ambient_lighting: AmbientLighting::default(),
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
            (InputButton::Key(KeyCode::Delete), true) => self.delete_selected_entity(),
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
        let shadow_camera = self.scene.shadow_camera(size.height)?;
        let material = self.selected_material_inspector();
        let light = self.selected_light_inspector();
        let mut ui_frame = self.ui.run(
            &self.window,
            self.editor.view_mode(),
            material,
            light,
            self.ambient_lighting,
        );

        if let Some(mode) = ui_frame.response.view_mode {
            self.editor.set_view_mode(mode);
        }
        if let Some(edit) = ui_frame.response.material_edit.take() {
            self.apply_material_edit(edit);
        }
        if let Some(edit) = ui_frame.response.light_edit.take() {
            self.apply_light_edit(edit);
        }
        if let Some(ambient_lighting) = ui_frame.response.ambient_lighting_edit.take() {
            self.ambient_lighting = ambient_lighting;
        }

        if let Some(kind) = ui_frame.response.add_light.take() {
            self.add_light(kind);
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
        let point_lights = self.scene.point_lights();
        let spot_lights = self.scene.spot_lights();
        let gizmo_draw = self.editor.gizmo_draw(&self.scene, size.height as f32);
        let light_markers = if self.editor.view_mode() == ViewMode::LightMarkers {
            self.editor
                .light_marker_draws(&self.scene, size.height as f32)
        } else {
            Vec::new()
        };

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
            shadow_camera,
            &renderables,
            &point_lights,
            &spot_lights,
            self.ambient_lighting,
            gizmo_draw,
            &light_markers,
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

    fn selected_light_inspector(&self) -> Option<LightInspectorData> {
        let entity = self.editor.selected_entity()?;
        let entity_name = self.scene.world.get::<&Name>(entity).ok()?.0.clone();

        if let Ok(light) = self.scene.world.get::<&SpotLight>(entity) {
            return Some(LightInspectorData {
                entity_name,
                is_spot: true,
                color: light.color.to_array(),
                intensity: light.intensity,
                range: light.range,
                inner_angle: light.inner_angle,
                outer_angle: light.outer_angle,
                shadow_resolution: self.renderer.shadow_resolution(entity, true),
                shadow_face_visibility: None,
                shadow_view_visibility: self.renderer.spot_shadow_visibility(entity),
            });
        }

        let light = self.scene.world.get::<&PointLight>(entity).ok()?;
        Some(LightInspectorData {
            entity_name,
            is_spot: false,
            color: light.color.to_array(),
            intensity: light.intensity,
            range: light.range,
            inner_angle: 0.0,
            outer_angle: 0.0,
            shadow_resolution: self.renderer.shadow_resolution(entity, false),
            shadow_face_visibility: self.renderer.point_shadow_face_visibility(entity),
            shadow_view_visibility: None,
        })
    }

    fn apply_light_edit(&mut self, edit: LightEdit) {
        let Some(entity) = self.editor.selected_entity() else {
            return;
        };

        match edit {
            LightEdit::Point {
                color,
                intensity,
                range,
            } => {
                if let Ok(mut light) = self.scene.world.get::<&mut PointLight>(entity) {
                    light.color = Vec3::from_array(color);
                    light.intensity = intensity;
                    light.range = range;
                }
            }

            LightEdit::Spot {
                color,
                intensity,
                range,
                inner_angle,
                outer_angle,
            } => {
                if let Ok(mut light) = self.scene.world.get::<&mut SpotLight>(entity) {
                    light.color = Vec3::from_array(color);
                    light.intensity = intensity;
                    light.range = range;
                    light.inner_angle = inner_angle;
                    light.outer_angle = outer_angle;
                }
            }
        }
    }

    fn add_light(&mut self, kind: AddLightKind) {
        let base_name = match kind {
            AddLightKind::Point => "PointLight",
            AddLightKind::Spot => "SpotLight",
        };

        let name = self.unique_entity_name(base_name);
        let transform = self.new_light_transform();

        let entity = match kind {
            AddLightKind::Point => {
                self.scene
                    .world
                    .spawn((Name(name), transform, PointLight::default()))
            }
            AddLightKind::Spot => {
                self.scene
                    .world
                    .spawn((Name(name), transform, SpotLight::default()))
            }
        };

        self.editor.select(Some(entity));
        self.request_redraw();
    }

    fn unique_entity_name(&self, prefix: &str) -> String {
        let mut query = self.scene.world.query::<&Name>();
        let names = query
            .iter()
            .map(|name| name.0.clone())
            .collect::<HashSet<_>>();

        let mut suffix = 1;
        loop {
            let candidate = format!("{prefix}{suffix}");
            if !names.contains(&candidate) {
                return candidate;
            }
            suffix += 1;
        }
    }

    fn new_light_transform(&self) -> Transform {
        let mut query = self.scene.world.query::<(&Camera, &Transform)>();
        query
            .iter()
            .next()
            .map(|(_, camera)| Transform {
                translation: camera.translation + camera.rotation * Vec3::NEG_Z * 2.0,
                rotation: camera.rotation,
                scale: Vec3::ONE,
            })
            .unwrap_or(Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            })
    }

    fn apply_material_edit(&mut self, edit: MaterialEdit) {
        let Some(material) = self.scene.materials.get_mut(&edit.material_id) else {
            return;
        };

        material.albedo = glam::Vec3::from_array(edit.albedo);
        material.emission = glam::Vec3::from_array(edit.emission);
    }

    fn delete_selected_entity(&mut self) {
        let Some(entity) = self.editor.selected_entity() else {
            return;
        };

        if self.scene.world.get::<&Camera>(entity).is_ok() {
            tracing::warn!("cannot delete the active camera");
            return;
        }

        if self.scene.world.despawn(entity).is_ok() {
            self.editor.select(None);
        }
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
