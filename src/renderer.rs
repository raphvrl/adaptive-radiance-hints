mod bindings;
mod depth;
mod mesh;
mod meshes;
mod passes;

use anyhow::Result;
use glam::Mat4;

use bindings::frame::{FrameBindings, FrameUniform};
use depth::DepthTarget;
use passes::geometry::GeometryPass;

pub struct Renderer {
    frame: FrameBindings,
    geometry: GeometryPass,
    depth: DepthTarget,
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

        Ok(Self {
            frame,
            geometry,
            depth,
        })
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth.resize(device, width, height);
    }

    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        view_proj: Mat4,
    ) -> Result<()> {
        self.frame.write(queue, &FrameUniform { view_proj })?;

        self.geometry
            .encode(encoder, view, self.depth.view(), self.frame.bind_group());

        Ok(())
    }
}
