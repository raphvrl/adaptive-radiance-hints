use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};
use wesl_core::include_wesl;
use wgpu::util::DeviceExt;

use crate::renderer::tiles::TileLists;

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CullParams {
    view_proj: [[f32; 4]; 4],
    viewport_tiles: [u32; 4],
}

pub struct CullingPass {
    pipeline: wgpu::ComputePipeline,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    depth_layout: wgpu::BindGroupLayout,
    depth_bind_group: wgpu::BindGroup,
}

impl CullingPass {
    pub fn new(
        device: &wgpu::Device,
        lights_layout: &wgpu::BindGroupLayout,
        tiles: &TileLists,
        depth_view: &wgpu::TextureView,
    ) -> Self {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tile culling params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(std::mem::size_of::<CullParams>() as u64),
                },
                count: None,
            }],
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tile culling depth layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let initial_params = CullParams {
            view_proj: glam::Mat4::IDENTITY.to_cols_array_2d(),
            viewport_tiles: [
                tiles.width(),
                tiles.height(),
                tiles.tiles_x(),
                tiles.tiles_y(),
            ],
        };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tile culling params"),
            contents: bytemuck::bytes_of(&initial_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tile culling params bind group"),
            layout: &params_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });
        let depth_bind_group = create_depth_bind_group(device, &depth_layout, depth_view);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Tile culling pipeline layout"),
            bind_group_layouts: &[
                Some(&params_layout),
                Some(lights_layout),
                Some(&depth_layout),
                Some(tiles.compute_layout()),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Tile culling shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("culling").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Tile culling pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: None,
        });

        Self {
            pipeline,
            params_buffer,
            params_bind_group,
            depth_layout,
            depth_bind_group,
        }
    }

    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view_proj: glam::Mat4,
        lights_bind_group: &wgpu::BindGroup,
        tiles: &TileLists,
    ) {
        let params = CullParams {
            view_proj: view_proj.to_cols_array_2d(),
            viewport_tiles: [
                tiles.width(),
                tiles.height(),
                tiles.tiles_x(),
                tiles.tiles_y(),
            ],
        };
        queue.write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&params));

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Tile light culling"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.params_bind_group, &[]);
        pass.set_bind_group(1, lights_bind_group, &[]);
        pass.set_bind_group(2, &self.depth_bind_group, &[]);
        pass.set_bind_group(3, tiles.compute_bind_group(), &[]);
        pass.dispatch_workgroups(tiles.tiles_x(), tiles.tiles_y(), 1);
    }

    pub fn set_depth_view(&mut self, device: &wgpu::Device, depth_view: &wgpu::TextureView) {
        self.depth_bind_group = create_depth_bind_group(device, &self.depth_layout, depth_view);
    }
}

fn create_depth_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    depth_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Tile culling depth bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(depth_view),
        }],
    })
}
