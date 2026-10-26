use winit::dpi::PhysicalSize;

use super::{Ui, UiFrame};

impl Ui {
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: PhysicalSize<u32>,
        frame: &mut UiFrame,
    ) -> Vec<wgpu::CommandBuffer> {
        let output = &mut frame.output;

        for (texture_id, deltas) in output.textures_delta.set.drain() {
            for delta in deltas {
                self.renderer
                    .update_texture(device, queue, texture_id, &delta);
            }
        }

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [size.width, size.height],
            pixels_per_point: output.pixels_per_point,
        };

        let paint_jobs = self
            .context
            .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);

        let callback_commands =
            self.renderer
                .update_buffers(device, queue, encoder, &paint_jobs, &screen);

        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            self.renderer
                .render(&mut pass.forget_lifetime(), &paint_jobs, &screen);
        }

        callback_commands
    }

    pub fn free_textures(&mut self, frame: &mut UiFrame) {
        for texture_id in frame.output.textures_delta.free.drain() {
            self.renderer.free_texture(&texture_id);
        }
    }
}
