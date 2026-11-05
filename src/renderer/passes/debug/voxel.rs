use bytemuck::{Pod, Zeroable};
use wesl_core::include_wesl;
use wgpu::util::DeviceExt;

use crate::renderer::{depth, voxel::VOXELS_PER_AXIS};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VoxelLineVertex {
    position: [f32; 3],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

const CUBE_LINE_VERTICES: [VoxelLineVertex; 24] = [
    VoxelLineVertex {
        position: [-1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, 1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, 1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, 1.0],
    },
    VoxelLineVertex {
        position: [-1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [-1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, -1.0, 1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [1.0, 1.0, 1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, -1.0],
    },
    VoxelLineVertex {
        position: [-1.0, 1.0, 1.0],
    },
];

pub struct VoxelDebugPass {
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    cascade_bind_groups: Vec<(wgpu::BindGroup, wgpu::BindGroup)>,
}

impl VoxelDebugPass {
    pub fn new<'a>(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        frame_layout: &wgpu::BindGroupLayout,
        cascade_buffers: impl Iterator<Item = (&'a wgpu::Buffer, &'a wgpu::Buffer)>,
    ) -> Self {
        let occupancy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel debug occupancy layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel debug params layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let cascade_bind_groups = cascade_buffers
            .map(|(occupancy_buffer, params_buffer)| {
                let occupancy = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Voxel debug occupancy bind group"),
                    layout: &occupancy_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: occupancy_buffer.as_entire_binding(),
                    }],
                });
                let params = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Voxel debug params bind group"),
                    layout: &params_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: params_buffer.as_entire_binding(),
                    }],
                });
                (occupancy, params)
            })
            .collect();
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Voxel debug shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("debug/voxel").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Voxel debug pipeline layout"),
            bind_group_layouts: &[
                Some(frame_layout),
                Some(&occupancy_layout),
                Some(&params_layout),
                Some(&params_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Voxel debug pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<VoxelLineVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth::FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Voxel debug cube lines"),
            contents: bytemuck::cast_slice(&CUBE_LINE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            cascade_bind_groups,
        }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        frame_bind_group: &wgpu::BindGroup,
        visible: bool,
    ) {
        if !visible {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Voxel debug pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        let voxel_count = (VOXELS_PER_AXIS * VOXELS_PER_AXIS * VOXELS_PER_AXIS) as u32;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, frame_bind_group, &[]);
        if let Some((_, params)) = self.cascade_bind_groups.first() {
            pass.set_bind_group(3, params, &[]);
        }
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        for (index, (occupancy, params)) in self.cascade_bind_groups.iter().enumerate() {
            pass.set_bind_group(1, occupancy, &[]);
            pass.set_bind_group(2, params, &[]);
            let first_instance = index as u32 * voxel_count;
            pass.draw(
                0..CUBE_LINE_VERTICES.len() as u32,
                first_instance..first_instance + voxel_count,
            );
        }
    }
}
