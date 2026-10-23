use std::num::NonZeroU64;

use anyhow::Result;
use encase::{ShaderType, UniformBuffer};
use glam::Mat4;
use wgpu::util::DeviceExt;

#[derive(ShaderType)]
pub(in crate::renderer) struct FrameUniform {
    pub(in crate::renderer) view_proj: Mat4,
}

impl Default for FrameUniform {
    fn default() -> Self {
        Self {
            view_proj: Mat4::IDENTITY,
        }
    }
}

pub(in crate::renderer) struct FrameBindings {
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl FrameBindings {
    pub(in crate::renderer) fn new(device: &wgpu::Device) -> Result<Self> {
        let initial_contents: Vec<u8> =
            UniformBuffer::<Vec<u8>>::content_of(&FrameUniform::default())?;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Frame bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(initial_contents.len() as u64),
                },
                count: None,
            }],
        });

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame uniform buffer"),
            contents: &initial_contents,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Frame bind group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            layout,
            buffer,
            bind_group,
        })
    }

    pub(in crate::renderer) fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub(in crate::renderer) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub(in crate::renderer) fn write(
        &self,
        queue: &wgpu::Queue,
        frame: &FrameUniform,
    ) -> Result<()> {
        let contents: Vec<u8> = UniformBuffer::<Vec<u8>>::content_of(frame)?;
        queue.write_buffer(&self.buffer, 0, &contents);
        Ok(())
    }
}
