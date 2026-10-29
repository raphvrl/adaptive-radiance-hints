mod assets;
mod bindings;
mod depth;
mod instances;
pub mod mesh;
pub mod meshes;
mod passes;

use anyhow::Result;
use glam::Mat4;

use crate::{editor::GizmoDraw, handle::MeshHandle, scene::MeshInstance};

use assets::MeshAssets;
use bindings::frame::{FrameBindings, FrameUniform};
use depth::DepthTarget;
use instances::InstanceBuffer;
use mesh::MeshData;
use passes::{geometry::GeometryPass, gizmo::GizmoPass, selection::SelectionPass};

pub struct Renderer {
    frame: FrameBindings,
    geometry: GeometryPass,
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
        let geometry = GeometryPass::new(device, target_format, frame.layout());
        let selection = SelectionPass::new(device, target_format, frame.layout());
        let gizmo = GizmoPass::new(device, target_format, frame.layout());
        let depth = DepthTarget::new(device, width, height);
        let instances = InstanceBuffer::new(device)?;

        Ok(Self {
            frame,
            geometry,
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
        self.depth.resize(device, width, height);
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        view_proj: Mat4,
        renderables: &[MeshInstance],
        gizmo_draw: Option<GizmoDraw>,
    ) -> Result<()> {
        self.frame.write(queue, &FrameUniform { view_proj })?;
        let batches = self.instances.write(device, queue, renderables)?;
        let selected_model = renderables
            .iter()
            .find_map(|instance| instance.selected.then_some(instance.model));

        self.geometry.encode(
            encoder,
            view,
            self.depth.view(),
            self.frame.bind_group(),
            &self.assets,
            &self.instances,
            &batches,
        )?;

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
        );

        Ok(())
    }
}
