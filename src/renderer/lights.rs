use anyhow::{Context, Result};
use encase::{ArrayLength, ShaderType, StorageBuffer};
use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::scene::{PointLightInstance, SpotLightInstance};

use super::shadows::ShadowPlan;

#[derive(ShaderType)]
struct GpuLight {
    position: Vec3,
    range: f32,

    direction: Vec3,
    inner_cos: f32,

    color: Vec3,
    intensity: f32,

    outer_cos: f32,
    is_spot: u32,
    shadow_index: u32,
}

#[derive(ShaderType)]
struct Lights {
    length: ArrayLength,
    #[shader(size(runtime))]
    lights: Vec<GpuLight>,
}

pub struct LightBuffer {
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: u64,
}

impl LightBuffer {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Light bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let contents = Self::serialize(&[], &[], None)?;
        let capacity = u64::try_from(contents.len()).context("light buffer size overflow")?;
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Light storage buffer"),
            contents: &contents,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = Self::create_bind_group(device, &layout, &buffer);

        Ok(Self {
            layout,
            buffer,
            bind_group,
            capacity,
        })
    }

    pub fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        point_lights: &[PointLightInstance],
        spot_lights: &[SpotLightInstance],
        shadow_plan: &ShadowPlan,
    ) -> Result<()> {
        let contents = Self::serialize(point_lights, spot_lights, Some(shadow_plan))?;
        let required = u64::try_from(contents.len()).context("light buffer size overflow")?;
        self.ensure_capacity(device, required)?;
        queue.write_buffer(&self.buffer, 0, &contents);
        Ok(())
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn serialize(
        point_lights: &[PointLightInstance],
        spot_lights: &[SpotLightInstance],
        shadow_plan: Option<&ShadowPlan>,
    ) -> Result<Vec<u8>> {
        let point_lights = point_lights.iter().map(|light| {
            Ok(GpuLight {
                position: light.position,
                range: light.range,

                direction: Vec3::ZERO,
                inner_cos: 0.0,

                color: light.color,
                intensity: light.intensity,

                outer_cos: 0.0,
                is_spot: 0,
                shadow_index: shadow_plan
                    .and_then(|plan| plan.point_tile(light.entity))
                    .context("point light has no shadow atlas allocation")?,
            })
        });

        let spot_lights = spot_lights.iter().map(|light| {
            Ok(GpuLight {
                position: light.position,
                range: light.range,

                direction: light.direction,
                inner_cos: light.inner_angle.cos(),

                color: light.color,
                intensity: light.intensity,

                outer_cos: light.outer_angle.cos(),
                is_spot: 1,
                shadow_index: shadow_plan
                    .and_then(|plan| plan.spot_tile(light.entity))
                    .context("spot light has no shadow atlas allocation")?,
            })
        });

        let lights = point_lights
            .chain(spot_lights)
            .collect::<Result<Vec<_>>>()?;
        let lights = Lights {
            length: ArrayLength,
            lights,
        };

        Ok(StorageBuffer::<Vec<u8>>::content_of(&lights)?)
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, required: u64) -> Result<()> {
        if required <= self.capacity {
            return Ok(());
        }

        let capacity = required
            .checked_next_power_of_two()
            .context("light buffer capacity overflow")?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Light storage buffer"),
            size: capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = Self::create_bind_group(device, &self.layout, &buffer);

        self.buffer = buffer;
        self.bind_group = bind_group;
        self.capacity = capacity;

        Ok(())
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Light bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }
}
