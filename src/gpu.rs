use std::sync::Arc;

use anyhow::{Context, Result, bail};
use winit::{dpi::PhysicalSize, window::Window};

pub struct Gpu {
    instance: wgpu::Instance,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    surface_configured: bool,
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(Arc::clone(&window))
            .context("failed to create surface")?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: true,
            })
            .await
            .context("failed to find a compatible graphics adapter")?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("GPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .context("failed to create GPU device")?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .or_else(|| capabilities.formats.iter().find(|format| !format.is_srgb()))
            .or_else(|| capabilities.formats.first())
            .copied()
            .context("surface has no supported texture formats")?;

        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .context("surface has no supported alpha modes")?;

        let present_mode = wgpu::PresentMode::Immediate;

        if !capabilities.present_modes.contains(&present_mode) {
            bail!("surface does not support immediate presentation");
        }

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };

        let surface_configured = size.width > 0 && size.height > 0;

        if surface_configured {
            surface.configure(&device, &config);
        }

        tracing::info!(adapter = %adapter.get_info().name, "initialized wgpu");

        Ok(Self {
            instance,
            window,
            surface,
            device,
            queue,
            config,
            surface_configured,
        })
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            self.surface_configured = false;
            return;
        }

        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.surface_configured = true;
    }

    pub fn acquire_frame(&mut self) -> Result<Option<(wgpu::SurfaceTexture, bool)>> {
        if !self.surface_configured {
            return Ok(None);
        }

        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Ok(Some((frame, false))),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Ok(Some((frame, true))),
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => Ok(None),
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                self.window.request_redraw();
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.recreate_surface()?;
                self.window.request_redraw();
                Ok(None)
            }
        }
    }

    pub fn present(&mut self, frame: wgpu::SurfaceTexture, suboptimal: bool) {
        self.queue.present(frame);

        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn recreate_surface(&mut self) -> Result<()> {
        self.surface = self
            .instance
            .create_surface(Arc::clone(&self.window))
            .context("failed to recreate surface")?;

        if self.config.width == 0 || self.config.height == 0 {
            bail!("cannot configure surface with a zero-sized window");
        }

        self.surface.configure(&self.device, &self.config);
        self.surface_configured = true;
        Ok(())
    }
}
