mod passes;

use passes::triangle::TrianglePass;

pub(crate) struct Renderer {
    triangle: TrianglePass,
}

impl Renderer {
    pub(crate) fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        Self {
            triangle: TrianglePass::new(device, target_format),
        }
    }

    pub(crate) fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
    ) {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render encoder"),
        });

        self.triangle.encode(&mut encoder, view);

        queue.submit([encoder.finish()]);
    }
}
