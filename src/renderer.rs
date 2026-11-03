mod assets;
mod bindings;
mod depth;
mod frustum;
mod hdr;
mod instances;
mod lights;
mod passes;
mod shadows;
mod tiles;

pub mod mesh;
pub mod meshes;

use anyhow::Result;
use glam::{Mat4, Vec3, Vec4};

use crate::{
    editor::{GizmoDraw, LightMarkerDraw},
    handle::MeshHandle,
    scene::{MeshInstance, PointLightInstance, SpotLightInstance},
};

use assets::MeshAssets;
use bindings::frame::{FrameBindings, FrameUniform};
use depth::DepthTarget;
use hdr::HdrTarget;
use instances::InstanceBuffer;
use lights::LightBuffer;
use mesh::MeshData;
use passes::{CullingPass, GeometryPass, GizmoPass, HeatmapPass, SelectionPass, TonemapPass};
use shadows::{ShadowAtlas, ShadowPass, ShadowPlan};
use tiles::TileLists;

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
    culling: CullingPass,
    heatmap: HeatmapPass,
    tiles: TileLists,
    tonemap: TonemapPass,
    selection: SelectionPass,
    gizmo: GizmoPass,
    depth: DepthTarget,
    shadow_atlas: ShadowAtlas,
    shadow_pass: ShadowPass,
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
        let hdr = HdrTarget::new(device, width, height);
        let shadow_atlas = ShadowAtlas::new(device)?;
        let depth = DepthTarget::new(device, width, height);
        let tiles = TileLists::new(device, width, height);
        let geometry = GeometryPass::new(
            device,
            hdr::FORMAT,
            frame.layout(),
            lights.layout(),
            shadow_atlas.layout(),
            tiles.geometry_layout(),
        );
        let culling = CullingPass::new(device, lights.layout(), &tiles, depth.view());
        let heatmap = HeatmapPass::new(device, target_format, tiles.geometry_layout());
        let tonemap = TonemapPass::new(device, target_format, hdr.view())?;
        let selection = SelectionPass::new(device, target_format, frame.layout());
        let gizmo = GizmoPass::new(device, target_format, frame.layout());
        let shadow_pass = ShadowPass::new(device)?;
        let instances = InstanceBuffer::new(device)?;

        Ok(Self {
            frame,
            lights,
            hdr,
            geometry,
            culling,
            heatmap,
            tiles,
            tonemap,
            selection,
            gizmo,
            depth,
            shadow_atlas,
            shadow_pass,
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
        self.tiles.resize(device, width, height);
        self.culling.set_depth_view(device, self.depth.view());
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
        show_tile_heatmap: bool,
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

        self.geometry.encode_depth(
            encoder,
            self.depth.view(),
            self.frame.bind_group(),
            &self.assets,
            &self.instances,
            &batches,
        )?;

        self.culling.encode(
            queue,
            encoder,
            view_proj,
            self.lights.bind_group(),
            &self.tiles,
        );

        self.geometry.encode(
            encoder,
            self.hdr.view(),
            self.depth.view(),
            self.frame.bind_group(),
            self.lights.bind_group(),
            self.shadow_atlas.bind_group(),
            self.tiles.geometry_bind_group(),
            &self.assets,
            &self.instances,
            &batches,
        )?;

        self.tonemap.encode(encoder, view);

        if show_tile_heatmap {
            self.heatmap
                .encode(encoder, view, self.tiles.geometry_bind_group());
        }

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
