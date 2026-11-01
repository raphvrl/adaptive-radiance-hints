mod assets;
mod bindings;
mod depth;
mod frustum;
mod hdr;
mod instances;
mod lights;
mod passes;
mod rsm;
mod shadows;

pub mod mesh;
pub mod meshes;

use anyhow::{Context, Result};
use glam::{Mat4, Vec3, Vec4};

use crate::{
    editor::{GizmoDraw, LightMarkerDraw},
    handle::MeshHandle,
    scene::{
        MeshInstance, PointLightInstance, SpotLightInstance, components::POINT_SHADOW_FACE_COUNT,
    },
};

use assets::MeshAssets;
use bindings::frame::{FrameBindings, FrameUniform};
use depth::DepthTarget;
use hdr::HdrTarget;
use instances::InstanceBuffer;
use lights::LightBuffer;
use mesh::MeshData;
use passes::{GeometryPass, GizmoPass, RsmPass, SelectionPass, TonemapPass};
use rsm::{RsmAtlas, RsmView, cull_empty_views};
use shadows::{ShadowAtlas, ShadowPass, ShadowPlan};

#[derive(Clone, Copy, Debug)]
pub struct AmbientLighting {
    pub color: Vec3,
    pub intensity: f32,
}

impl Default for AmbientLighting {
    fn default() -> Self {
        Self {
            color: Vec3::ONE,
            intensity: 0.1,
        }
    }
}

pub struct Renderer {
    frame: FrameBindings,
    lights: LightBuffer,
    hdr: HdrTarget,
    geometry: GeometryPass,
    tonemap: TonemapPass,
    selection: SelectionPass,
    gizmo: GizmoPass,
    depth: DepthTarget,
    shadow_atlas: ShadowAtlas,
    shadow_pass: ShadowPass,
    rsm_atlas: RsmAtlas,
    rsm_pass: RsmPass,
    last_shadow_plan: Option<ShadowPlan>,
    assets: MeshAssets,
    instances: InstanceBuffer,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let frame = FrameBindings::new(device)?;
        let lights = LightBuffer::new(device)?;
        let rsm_atlas = RsmAtlas::new(device);
        let rsm_pass = RsmPass::new(device, lights.layout())?;
        let hdr = HdrTarget::new(device, width, height);
        let shadow_atlas = ShadowAtlas::new(device)?;
        let geometry = GeometryPass::new(
            device,
            hdr::FORMAT,
            frame.layout(),
            lights.layout(),
            shadow_atlas.layout(),
        );
        let tonemap = TonemapPass::new(device, target_format, hdr.view())?;
        let selection = SelectionPass::new(device, target_format, frame.layout());
        let gizmo = GizmoPass::new(device, target_format, frame.layout());
        let depth = DepthTarget::new(device, width, height);
        let shadow_pass = ShadowPass::new(device)?;
        let instances = InstanceBuffer::new(device)?;

        Ok(Self {
            frame,
            lights,
            hdr,
            geometry,
            tonemap,
            selection,
            gizmo,
            depth,
            shadow_atlas,
            shadow_pass,
            rsm_atlas,
            rsm_pass,
            last_shadow_plan: None,
            assets: MeshAssets::default(),
            instances,
        })
    }

    pub fn register_mesh(&mut self, device: &wgpu::Device, data: &MeshData) -> Result<MeshHandle> {
        self.assets.insert(device, data)
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.depth.resize(device, width, height);
        self.hdr.resize(device, width, height);
        self.tonemap.set_hdr_view(device, self.hdr.view());
    }

    pub fn point_shadow_face_visibility(&self, entity: hecs::Entity) -> Option<[bool; 6]> {
        self.last_shadow_plan
            .as_ref()?
            .point_face_camera_visibility(entity)
    }

    pub fn spot_shadow_visibility(&self, entity: hecs::Entity) -> Option<bool> {
        self.last_shadow_plan
            .as_ref()?
            .spot_camera_visibility(entity)
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        view_proj: Mat4,
        camera_position: Vec3,
        renderables: &[MeshInstance],
        point_lights: &[PointLightInstance],
        spot_lights: &[SpotLightInstance],
        ambient_lighting: AmbientLighting,
        gizmo_draw: Option<GizmoDraw>,
        light_markers: &[LightMarkerDraw],
    ) -> Result<()> {
        self.frame.write(
            queue,
            &FrameUniform {
                view_proj,
                ambient_light: Vec4::new(
                    ambient_lighting.color.x,
                    ambient_lighting.color.y,
                    ambient_lighting.color.z,
                    ambient_lighting.intensity,
                ),
                camera_position: camera_position.extend(1.0),
            },
        )?;
        let batches = self.instances.write(device, queue, renderables)?;
        let selected_model = renderables
            .iter()
            .find_map(|instance| instance.selected.then_some(instance.model));
        let world_bounds = renderables
            .iter()
            .map(|instance| {
                self.assets
                    .get(instance.mesh)?
                    .bounds()
                    .transformed(instance.model)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut shadow_plan =
            shadows::build_plan(point_lights, spot_lights, view_proj, &world_bounds)?;
        self.shadow_atlas
            .ensure_capacity(device, &mut shadow_plan.views)?;
        self.shadow_atlas
            .write_views(device, queue, &shadow_plan.views)?;
        self.lights
            .write(device, queue, point_lights, spot_lights, &shadow_plan)?;
        self.shadow_pass.encode(
            device,
            queue,
            encoder,
            self.shadow_atlas.view(),
            &self.assets,
            &self.instances,
            &batches,
            &mut shadow_plan.views,
        )?;

        let rsm_view_count = point_lights
            .len()
            .checked_mul(POINT_SHADOW_FACE_COUNT)
            .and_then(|count| count.checked_add(spot_lights.len()))
            .context("RSM view count overflow")?;
        let mut rsm_views = Vec::with_capacity(rsm_view_count);

        for (light_index, light) in point_lights.iter().enumerate() {
            let first_view = shadow_plan
                .point_tile(light.entity)
                .context("point light has no RSM views")?;
            let first_view = usize::try_from(first_view).context("RSM view index overflow")?;
            let light_index = u32::try_from(light_index)?;
            let resolution = (light.shadow_resolution / 8).max(64);

            for face_index in 0..POINT_SHADOW_FACE_COUNT {
                let view_index = first_view
                    .checked_add(face_index)
                    .context("point light RSM view index overflow")?;
                let shadow_view = shadow_plan
                    .views
                    .get(view_index)
                    .context("point light RSM face is missing")?;

                rsm_views.push(RsmView {
                    view_proj: shadow_view.view_proj,
                    light_index,
                    resolution,
                    x: 0,
                    y: 0,
                    active: false,
                });
            }
        }

        let first_spot_index = u32::try_from(point_lights.len())?;
        for (spot_index, light) in spot_lights.iter().enumerate() {
            let view_index = shadow_plan
                .spot_tile(light.entity)
                .context("spot light has no RSM view")?;
            let view_index = usize::try_from(view_index).context("RSM view index overflow")?;
            let shadow_view = shadow_plan
                .views
                .get(view_index)
                .context("spot light RSM view is missing")?;
            let light_index = first_spot_index
                .checked_add(u32::try_from(spot_index)?)
                .context("RSM light index overflow")?;

            rsm_views.push(RsmView {
                view_proj: shadow_view.view_proj,
                light_index,
                resolution: (light.shadow_resolution / 8).max(64),
                x: 0,
                y: 0,
                active: false,
            });
        }

        cull_empty_views(&mut rsm_views, &world_bounds)?;
        self.rsm_atlas.ensure_capacity(device, &mut rsm_views)?;

        if !rsm_views.is_empty() {
            self.rsm_pass.encode(
                device,
                queue,
                encoder,
                &self.rsm_atlas,
                self.lights.bind_group(),
                &rsm_views,
                &self.assets,
                &self.instances,
                &batches,
            )?;
        }

        self.geometry.encode(
            encoder,
            self.hdr.view(),
            self.depth.view(),
            self.frame.bind_group(),
            self.lights.bind_group(),
            self.shadow_atlas.bind_group(),
            &self.assets,
            &self.instances,
            &batches,
        )?;

        self.tonemap.encode(encoder, view);

        self.selection.encode(
            queue,
            encoder,
            view,
            self.depth.view(),
            self.frame.bind_group(),
            selected_model,
        );

        self.gizmo.encode(
            device,
            queue,
            encoder,
            view,
            self.frame.bind_group(),
            gizmo_draw,
            light_markers,
        );

        self.last_shadow_plan = Some(shadow_plan);

        Ok(())
    }
}
