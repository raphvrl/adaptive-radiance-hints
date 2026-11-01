use std::num::NonZeroU64;

use anyhow::{Context, Result, ensure};
use encase::{ShaderType, UniformBuffer};
use glam::Mat4;
use wesl_core::include_wesl;

use crate::renderer::{
    assets::MeshAssets,
    instances::{InstanceBuffer, InstanceRaw, MeshBatch},
    mesh::MeshVertex,
    rsm::{COLOR_FORMAT, DEPTH_FORMAT, GUTTER, RsmAtlas, RsmView},
};

#[derive(ShaderType)]
struct RsmFrameUniform {
    view_proj: Mat4,
    light_index: u32,
}

pub struct RsmPass {
    pipeline: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    frame_buffer: wgpu::Buffer,
    frame_bind_group: wgpu::BindGroup,
    frame_size: NonZeroU64,
    frame_stride: u64,
    frame_capacity: u32,
}

impl RsmPass {
    pub fn new(device: &wgpu::Device, lights_layout: &wgpu::BindGroupLayout) -> Result<Self> {
        let initial_contents = Self::serialize_frame(Mat4::IDENTITY, 0)?;
        let frame_size = NonZeroU64::new(
            u64::try_from(initial_contents.len()).context("RSM frame size overflow")?,
        )
        .context("RSM frame uniform cannot be empty")?;
        let frame_stride = aligned_stride(
            frame_size.get(),
            u64::from(device.limits().min_uniform_buffer_offset_alignment),
        )?;
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RSM frame bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(frame_size),
                },
                count: None,
            }],
        });

        let frame_buffer = Self::create_frame_buffer(device, frame_stride);
        let frame_bind_group =
            Self::create_frame_bind_group(device, &frame_layout, &frame_buffer, frame_size);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RSM shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("rsm").into()),
        });

        let bind_group_layouts = [Some(&frame_layout), Some(lights_layout)];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RSM pipeline layout"),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("RSM pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                buffers: &[Some(MeshVertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            pipeline,
            frame_layout,
            frame_buffer,
            frame_bind_group,
            frame_size,
            frame_stride,
            frame_capacity: 1,
        })
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        atlas: &RsmAtlas,
        lights_bind_group: &wgpu::BindGroup,
        views: &[RsmView],
        assets: &MeshAssets,
        instances: &InstanceBuffer,
        batches: &[MeshBatch],
    ) -> Result<()> {
        let active_count = views.iter().filter(|view| view.active).count();
        if active_count == 0 {
            return Ok(());
        }

        let required = u32::try_from(active_count).context("too many RSM views")?;
        self.ensure_frame_capacity(device, required)?;

        let write_size = self
            .frame_stride
            .checked_mul(u64::try_from(active_count).context("RSM view count overflow")?)
            .context("RSM frame buffer write size overflow")?;
        let write_size =
            usize::try_from(write_size).context("RSM frame buffer write size overflow")?;
        let mut contents = vec![0; write_size];

        for (index, view) in views.iter().filter(|view| view.active).enumerate() {
            let frame_contents = Self::serialize_frame(view.view_proj, view.light_index)?;
            ensure!(
                u64::try_from(frame_contents.len()).context("RSM frame size overflow")?
                    == self.frame_size.get(),
                "serialized RSM frame has an unexpected size"
            );

            let offset = u64::try_from(index)
                .context("RSM frame index overflow")?
                .checked_mul(self.frame_stride)
                .context("RSM frame offset overflow")?;
            let start = usize::try_from(offset).context("RSM frame offset overflow")?;
            let end = start
                .checked_add(frame_contents.len())
                .context("RSM frame range overflow")?;
            contents[start..end].copy_from_slice(&frame_contents);
        }

        queue.write_buffer(&self.frame_buffer, 0, &contents);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("RSM atlas pass"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: atlas.normal_view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: atlas.flux_view(),
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: atlas.depth_view(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, lights_bind_group, &[]);
        pass.set_vertex_buffer(1, instances.buffer().slice(..));

        for (view_index, view) in views.iter().filter(|view| view.active).enumerate() {
            ensure!(view.resolution > 0, "RSM view resolution cannot be zero");
            let tile_size = view
                .resolution
                .checked_add(2 * GUTTER)
                .context("RSM tile size overflow")?;
            let tile_x = view.x;
            let tile_y = view.y;
            let tile_right = tile_x
                .checked_add(tile_size)
                .context("RSM tile bounds overflow")?;
            let tile_bottom = tile_y
                .checked_add(tile_size)
                .context("RSM tile bounds overflow")?;
            ensure!(
                tile_right <= atlas.width() && tile_bottom <= atlas.height(),
                "RSM view tile is outside the atlas"
            );

            let dynamic_offset = u32::try_from(
                u64::try_from(view_index)
                    .context("RSM frame index overflow")?
                    .checked_mul(self.frame_stride)
                    .context("RSM frame offset overflow")?,
            )
            .context("RSM dynamic offset overflow")?;

            pass.set_bind_group(0, &self.frame_bind_group, &[dynamic_offset]);
            pass.set_viewport(
                (tile_x + GUTTER) as f32,
                (tile_y + GUTTER) as f32,
                view.resolution as f32,
                view.resolution as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(tile_x, tile_y, tile_size, tile_size);

            for batch in batches {
                let mesh = assets.get(batch.mesh)?;
                let end_instance = batch.first_instance + batch.instance_count;

                pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
                pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count(), 0, batch.first_instance..end_instance);
            }
        }

        Ok(())
    }

    fn ensure_frame_capacity(&mut self, device: &wgpu::Device, required: u32) -> Result<()> {
        if required <= self.frame_capacity {
            return Ok(());
        }

        let capacity = required
            .checked_next_power_of_two()
            .context("RSM frame buffer capacity overflow")?;
        let size = self
            .frame_stride
            .checked_mul(u64::from(capacity))
            .context("RSM frame buffer size overflow")?;
        ensure!(
            size <= device.limits().max_buffer_size,
            "RSM frame buffer exceeds the GPU's maximum buffer size"
        );

        let frame_buffer = Self::create_frame_buffer(device, size);
        let frame_bind_group = Self::create_frame_bind_group(
            device,
            &self.frame_layout,
            &frame_buffer,
            self.frame_size,
        );

        self.frame_buffer = frame_buffer;
        self.frame_bind_group = frame_bind_group;
        self.frame_capacity = capacity;

        Ok(())
    }

    fn create_frame_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RSM frame uniform buffer"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_frame_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        frame_size: NonZeroU64,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RSM frame bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: Some(frame_size),
                }),
            }],
        })
    }

    fn serialize_frame(view_proj: Mat4, light_index: u32) -> Result<Vec<u8>> {
        Ok(UniformBuffer::<Vec<u8>>::content_of(&RsmFrameUniform {
            view_proj,
            light_index,
        })?)
    }
}

fn aligned_stride(size: u64, alignment: u64) -> Result<u64> {
    ensure!(alignment > 0, "RSM uniform alignment cannot be zero");

    let remainder = size % alignment;
    if remainder == 0 {
        return Ok(size);
    }

    size.checked_add(alignment - remainder)
        .context("RSM uniform stride overflow")
}
