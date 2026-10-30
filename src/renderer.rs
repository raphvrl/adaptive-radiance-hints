mod assets;
mod bindings;
mod depth;
mod hdr;
mod instances;
mod lights;
mod passes;

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
use passes::{GeometryPass, GizmoPass, SelectionPass, TonemapPass};

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
        let geometry = GeometryPass::new(device, hdr::FORMAT, frame.layout(), lights.layout());
        let tonemap = TonemapPass::new(device, target_format, hdr.view())?;
        let selection = SelectionPass::new(device, target_format, frame.layout());
        let gizmo = GizmoPass::new(device, target_format, frame.layout());
        let depth = DepthTarget::new(device, width, height);
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

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        view_proj: Mat4,
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
            },
        )?;
        self.lights
            .write(device, queue, point_lights, spot_lights)?;
        let batches = self.instances.write(device, queue, renderables)?;
        let selected_model = renderables
            .iter()
            .find_map(|instance| instance.selected.then_some(instance.model));

        self.geometry.encode(
            encoder,
            self.hdr.view(),
            self.depth.view(),
            self.frame.bind_group(),
            self.lights.bind_group(),
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

        Ok(())
    }
}
