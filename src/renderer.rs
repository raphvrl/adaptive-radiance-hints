mod bindings;
mod passes;

use anyhow::Result;
use bindings::frame::{FrameBindings, FrameUniform};
use glam::Mat4;
use passes::triangle::TrianglePass;

pub struct Renderer {
    frame: FrameBindings,
    triangle: TrianglePass,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Result<Self> {
        let frame = FrameBindings::new(device)?;
        let triangle = TrianglePass::new(device, target_format, frame.layout());

        Ok(Self { frame, triangle })
    }

    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        view_proj: Mat4,
    ) -> Result<()> {
        self.frame.write(queue, &FrameUniform { view_proj })?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render encoder"),
        });

        self.triangle
            .encode(&mut encoder, view, self.frame.bind_group());

        queue.submit([encoder.finish()]);

        Ok(())
    }
}
