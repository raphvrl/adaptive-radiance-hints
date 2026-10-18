use std::sync::Arc;

use anyhow::Result;
use winit::{dpi::PhysicalSize, window::Window};

use crate::{gpu::Gpu, renderer::Renderer};

pub struct State {
    window: Arc<Window>,
    gpu: Gpu,
    renderer: Renderer,
}

impl State {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let gpu = Gpu::new(Arc::clone(&window)).await?;

        let renderer = Renderer::new(gpu.device(), gpu.surface_format());

        Ok(Self {
            window,
            gpu,
            renderer,
        })
    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        self.gpu.resize(size);
    }

    pub fn render(&mut self) -> Result<()> {
        let Some((frame, suboptimal)) = self.gpu.acquire_frame()? else {
            return Ok(());
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.renderer
            .render(self.gpu.device(), self.gpu.queue(), &view);

        self.gpu.present(frame, suboptimal);

        Ok(())
    }
}
