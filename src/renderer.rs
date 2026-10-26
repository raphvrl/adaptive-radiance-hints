mod assets;
mod bindings;
mod depth;
mod instances;
pub(crate) mod mesh;
pub(crate) mod meshes;
mod passes;

use anyhow::Result;
use glam::Mat4;

use crate::handle::MeshHandle;

use assets::MeshAssets;
use bindings::frame::{FrameBindings, FrameUniform};
use depth::DepthTarget;
use instances::InstanceBuffer;
use mesh::MeshData;
use passes::geometry::GeometryPass;

pub struct Renderer {
    frame: FrameBindings,
    geometry: GeometryPass,
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
        let depth = DepthTarget::new(device, width, height);
        let instances = InstanceBuffer::new(device)?;

        Ok(Self {
            frame,
            geometry,
            depth,
            assets: MeshAssets::default(),
            instances,
        })
    }

    pub(crate) fn register_mesh(
        &mut self,
        device: &wgpu::Device,
        data: &MeshData,
    ) -> Result<MeshHandle> {
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
        renderables: &[(MeshHandle, Mat4)],
    ) -> Result<()> {
        self.frame.write(queue, &FrameUniform { view_proj })?;
        let batches = self.instances.write(device, queue, renderables)?;

        self.geometry.encode(
            encoder,
            view,
            self.depth.view(),
            self.frame.bind_group(),
            &self.assets,
            &self.instances,
            &batches,
        )?;

        Ok(())
    }
}
