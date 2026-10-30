use wesl_core::include_wesl;

use crate::renderer::{
    assets::MeshAssets,
    instances::{InstanceBuffer, InstanceRaw, MeshBatch},
    mesh::MeshVertex,
};

pub struct GeometryPass {
    pipeline: wgpu::RenderPipeline,
}

impl GeometryPass {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        frame_layout: &wgpu::BindGroupLayout,
        lights_layout: &wgpu::BindGroupLayout,
        shadows_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Geometry shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("geometry").into()),
        });

        let bind_group_layouts = [
            Some(frame_layout),
            Some(lights_layout),
            Some(shadows_layout),
        ];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Geometry pipeline layout"),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Geometry pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                buffers: &[Some(MeshVertex::LAYOUT), Some(InstanceRaw::LAYOUT)],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: crate::renderer::depth::FORMAT,
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
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline }
    }

    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        frame_bind_group: &wgpu::BindGroup,
        lights_bind_group: &wgpu::BindGroup,
        shadows_bind_group: &wgpu::BindGroup,
        assets: &MeshAssets,
        instances: &InstanceBuffer,
        batches: &[MeshBatch],
    ) -> anyhow::Result<()> {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Geometry pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
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
        pass.set_bind_group(0, frame_bind_group, &[]);
        pass.set_bind_group(1, lights_bind_group, &[]);
        pass.set_bind_group(2, shadows_bind_group, &[]);
        pass.set_vertex_buffer(1, instances.buffer().slice(..));

        for batch in batches {
            let mesh = assets.get(batch.mesh)?;
            let end_instance = batch.first_instance + batch.instance_count;

            pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count(), 0, batch.first_instance..end_instance);
        }

        Ok(())
    }
}
