use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wesl_core::include_wesl;

use crate::editor::{Axis, GizmoDraw, GizmoHandle, LightGizmo, LightMarkerDraw, TransformMode};

const RING_SEGMENTS: usize = 64;
const TUBE_SEGMENTS: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GizmoVertex {
    position: [f32; 3],
    color: [f32; 4],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 2] =
    wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

pub struct GizmoPass {
    pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    capacity: usize,
}

impl GizmoPass {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        frame_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Gizmo shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("gizmo").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Gizmo pipeline layout"),
            bind_group_layouts: &[Some(frame_layout)],
            immediate_size: 0,
        });

        let pipeline = create_pipeline(
            device,
            &shader,
            &pipeline_layout,
            target_format,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let line_pipeline = create_pipeline(
            device,
            &shader,
            &pipeline_layout,
            target_format,
            wgpu::PrimitiveTopology::LineList,
        );

        let capacity = 1024;
        let vertex_buffer = create_vertex_buffer(device, capacity);

        Self {
            pipeline,
            line_pipeline,
            vertex_buffer,
            capacity,
        }
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        frame_bind_group: &wgpu::BindGroup,
        gizmo: Option<GizmoDraw>,
        light_markers: &[LightMarkerDraw],
    ) {
        let mut vertices = gizmo.map(build_gizmo_vertices).unwrap_or_default();
        let line_start = vertices.len();

        if let Some(gizmo) = gizmo {
            if let Some(light) = gizmo.light {
                append_light_gizmo(&mut vertices, gizmo.center, gizmo.size, light);
            }
        }

        for marker in light_markers {
            append_light_marker(&mut vertices, *marker);
        }

        if vertices.is_empty() {
            return;
        }

        self.ensure_capacity(device, vertices.len());
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Gizmo pass"),
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

        pass.set_bind_group(0, frame_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));

        if line_start > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.draw(0..line_start as u32, 0..1);
        }

        if line_start < vertices.len() {
            pass.set_pipeline(&self.line_pipeline);
            pass.draw(line_start as u32..vertices.len() as u32, 0..1);
        }
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, required: usize) {
        if required <= self.capacity {
            return;
        }

        self.capacity = required.next_power_of_two();
        self.vertex_buffer = create_vertex_buffer(device, self.capacity);
    }
}

fn create_vertex_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Gizmo vertex buffer"),
        size: (capacity * std::mem::size_of::<GizmoVertex>()) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    target_format: wgpu::TextureFormat,
    topology: wgpu::PrimitiveTopology,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Gizmo pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: None,
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<GizmoVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VERTEX_ATTRIBUTES,
            })],
        },
        primitive: wgpu::PrimitiveState {
            topology,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}

fn build_gizmo_vertices(gizmo: GizmoDraw) -> Vec<GizmoVertex> {
    let mut vertices = Vec::with_capacity(8_192);

    match gizmo.mode {
        TransformMode::Translate => {
            for axis in Axis::ALL {
                let direction = axis.vector();
                let color = axis_color(axis, gizmo.active_handle);
                let shaft_end = gizmo.center + direction * (gizmo.size * 0.76);
                append_cylinder(
                    &mut vertices,
                    gizmo.center + direction * (gizmo.size * 0.08),
                    shaft_end,
                    direction,
                    gizmo.size * 0.018,
                    color,
                    12,
                );
                append_cone(
                    &mut vertices,
                    shaft_end,
                    gizmo.center + direction * gizmo.size,
                    direction,
                    gizmo.size * 0.065,
                    color,
                    12,
                );
            }
        }
        TransformMode::Rotate => {
            for axis in Axis::ALL {
                append_ring(
                    &mut vertices,
                    gizmo.center,
                    axis.vector(),
                    gizmo.size * 0.84,
                    gizmo.size * 0.014,
                    axis_color(axis, gizmo.active_handle),
                );
            }
        }
        TransformMode::Scale => {
            for axis in Axis::ALL {
                let direction = axis.vector();
                let color = axis_color(axis, gizmo.active_handle);
                let shaft_end = gizmo.center + direction * (gizmo.size * 0.84);
                append_cylinder(
                    &mut vertices,
                    gizmo.center + direction * (gizmo.size * 0.08),
                    shaft_end,
                    direction,
                    gizmo.size * 0.016,
                    color,
                    10,
                );
                append_cube(
                    &mut vertices,
                    gizmo.center + direction * (gizmo.size * 0.91),
                    gizmo.size * 0.045,
                    color,
                );
            }
            append_cube(
                &mut vertices,
                gizmo.center,
                gizmo.size * 0.06,
                center_handle_color(gizmo.active_handle),
            );
        }
    }

    vertices
}

fn append_light_gizmo(
    vertices: &mut Vec<GizmoVertex>,
    center: Vec3,
    gizmo_size: f32,
    light: LightGizmo,
) {
    let (range, intensity, light_color) = match light {
        LightGizmo::Point {
            range,
            intensity,
            color,
        }
        | LightGizmo::Spot {
            range,
            intensity,
            color,
            ..
        } => (range, intensity.max(0.0), color),
    };

    let power = intensity / (intensity + 10.0);
    let brightness = 0.25 + 0.75 * power;
    let display_color = (light_color * brightness).clamp(Vec3::splat(0.1), Vec3::ONE);
    let color = [display_color.x, display_color.y, display_color.z, 1.0];
    let marker_size = gizmo_size * (0.035 + 0.035 * power);

    append_wire_cube(vertices, center, marker_size, color);

    match light {
        LightGizmo::Point { .. } => {
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                append_wire_ring(vertices, center, axis, range, color);
            }
        }
        LightGizmo::Spot {
            direction,
            inner_angle,
            outer_angle,
            ..
        } => {
            let Some(direction) = direction.try_normalize() else {
                return;
            };
            let (u, v) = perpendicular_basis(direction);
            let end = center + direction * range;
            let outer_radius = range * outer_angle.tan();
            let inner_radius = range * inner_angle.tan();

            append_wire_ring(vertices, end, direction, outer_radius, color);
            if inner_radius > f32::EPSILON {
                let inner_color = [1.0, 0.88, 0.42, 1.0];
                append_wire_ring(vertices, end, direction, inner_radius, inner_color);
            }

            for segment in 0..8 {
                let angle = std::f32::consts::TAU * segment as f32 / 8.0;
                let radial = u * angle.cos() + v * angle.sin();
                append_line(vertices, center, end + radial * outer_radius, color);
            }
        }
    }
}

fn append_light_marker(vertices: &mut Vec<GizmoVertex>, marker: LightMarkerDraw) {
    let intensity = marker.intensity.max(0.0);
    let power = intensity / (intensity + 10.0);
    let brightness = 0.25 + 0.75 * power;
    let display_color = (marker.color * brightness).clamp(Vec3::splat(0.1), Vec3::ONE);
    let color = [display_color.x, display_color.y, display_color.z, 1.0];
    let marker_size = marker.size * (0.035 + 0.035 * power);

    append_wire_cube(vertices, marker.center, marker_size, color);
}

fn append_wire_ring(
    vertices: &mut Vec<GizmoVertex>,
    center: Vec3,
    axis: Vec3,
    radius: f32,
    color: [f32; 4],
) {
    let (u, v) = perpendicular_basis(axis);
    let mut previous = center + u * radius;

    for segment in 1..=RING_SEGMENTS {
        let angle = std::f32::consts::TAU * segment as f32 / RING_SEGMENTS as f32;
        let radial = u * angle.cos() + v * angle.sin();
        let next = center + radial * radius;
        append_line(vertices, previous, next, color);
        previous = next;
    }
}

fn append_wire_cube(
    vertices: &mut Vec<GizmoVertex>,
    center: Vec3,
    half_size: f32,
    color: [f32; 4],
) {
    const EDGES: [[usize; 2]; 12] = [
        [0, 1],
        [1, 2],
        [2, 3],
        [3, 0],
        [4, 5],
        [5, 6],
        [6, 7],
        [7, 4],
        [0, 4],
        [1, 5],
        [2, 6],
        [3, 7],
    ];
    let corners = [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ]
    .map(|corner| center + corner * half_size);

    for [start, end] in EDGES {
        append_line(vertices, corners[start], corners[end], color);
    }
}

fn append_line(vertices: &mut Vec<GizmoVertex>, start: Vec3, end: Vec3, color: [f32; 4]) {
    vertices.extend([start, end].map(|position| GizmoVertex {
        position: position.to_array(),
        color,
    }));
}

fn axis_color(axis: Axis, active_handle: Option<GizmoHandle>) -> [f32; 4] {
    if active_handle == Some(GizmoHandle::Axis(axis)) {
        return [1.0, 0.9, 0.15, 1.0];
    }

    match axis {
        Axis::X => [0.95, 0.22, 0.19, 1.0],
        Axis::Y => [0.28, 0.9, 0.3, 1.0],
        Axis::Z => [0.25, 0.5, 1.0, 1.0],
    }
}

fn center_handle_color(active_handle: Option<GizmoHandle>) -> [f32; 4] {
    if active_handle == Some(GizmoHandle::Center) {
        [1.0, 0.9, 0.15, 1.0]
    } else {
        [0.92, 0.92, 0.92, 1.0]
    }
}

fn append_cylinder(
    vertices: &mut Vec<GizmoVertex>,
    start: Vec3,
    end: Vec3,
    axis: Vec3,
    radius: f32,
    color: [f32; 4],
    segments: usize,
) {
    let (u, v) = perpendicular_basis(axis);

    for segment in 0..segments {
        let angle_a = std::f32::consts::TAU * segment as f32 / segments as f32;
        let angle_b = std::f32::consts::TAU * (segment + 1) as f32 / segments as f32;
        let radial_a = u * angle_a.cos() + v * angle_a.sin();
        let radial_b = u * angle_b.cos() + v * angle_b.sin();
        let start_a = start + radial_a * radius;
        let start_b = start + radial_b * radius;
        let end_a = end + radial_a * radius;
        let end_b = end + radial_b * radius;

        append_triangle(vertices, start_a, start_b, end_b, color);
        append_triangle(vertices, start_a, end_b, end_a, color);
    }
}

fn append_cone(
    vertices: &mut Vec<GizmoVertex>,
    base: Vec3,
    tip: Vec3,
    axis: Vec3,
    radius: f32,
    color: [f32; 4],
    segments: usize,
) {
    let (u, v) = perpendicular_basis(axis);

    for segment in 0..segments {
        let angle_a = std::f32::consts::TAU * segment as f32 / segments as f32;
        let angle_b = std::f32::consts::TAU * (segment + 1) as f32 / segments as f32;
        let point_a = base + (u * angle_a.cos() + v * angle_a.sin()) * radius;
        let point_b = base + (u * angle_b.cos() + v * angle_b.sin()) * radius;
        append_triangle(vertices, point_a, point_b, tip, color);
        append_triangle(vertices, base, point_b, point_a, color);
    }
}

fn append_ring(
    vertices: &mut Vec<GizmoVertex>,
    center: Vec3,
    axis: Vec3,
    radius: f32,
    tube_radius: f32,
    color: [f32; 4],
) {
    let (u, v) = perpendicular_basis(axis);

    for segment in 0..RING_SEGMENTS {
        let angle_a = std::f32::consts::TAU * segment as f32 / RING_SEGMENTS as f32;
        let angle_b = std::f32::consts::TAU * (segment + 1) as f32 / RING_SEGMENTS as f32;

        for tube_segment in 0..TUBE_SEGMENTS {
            let tube_angle_a = std::f32::consts::TAU * tube_segment as f32 / TUBE_SEGMENTS as f32;
            let tube_angle_b =
                std::f32::consts::TAU * (tube_segment + 1) as f32 / TUBE_SEGMENTS as f32;
            let point_aa = ring_point(
                center,
                u,
                v,
                axis,
                radius,
                tube_radius,
                angle_a,
                tube_angle_a,
            );
            let point_ba = ring_point(
                center,
                u,
                v,
                axis,
                radius,
                tube_radius,
                angle_b,
                tube_angle_a,
            );
            let point_bb = ring_point(
                center,
                u,
                v,
                axis,
                radius,
                tube_radius,
                angle_b,
                tube_angle_b,
            );
            let point_ab = ring_point(
                center,
                u,
                v,
                axis,
                radius,
                tube_radius,
                angle_a,
                tube_angle_b,
            );

            append_triangle(vertices, point_aa, point_ba, point_bb, color);
            append_triangle(vertices, point_aa, point_bb, point_ab, color);
        }
    }
}

fn ring_point(
    center: Vec3,
    u: Vec3,
    v: Vec3,
    axis: Vec3,
    radius: f32,
    tube_radius: f32,
    angle: f32,
    tube_angle: f32,
) -> Vec3 {
    let radial = u * angle.cos() + v * angle.sin();
    center
        + radial * (radius + tube_radius * tube_angle.cos())
        + axis * (tube_radius * tube_angle.sin())
}

fn append_cube(vertices: &mut Vec<GizmoVertex>, center: Vec3, half_size: f32, color: [f32; 4]) {
    const FACES: [[usize; 4]; 6] = [
        [0, 1, 2, 3],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 5, 6, 2],
        [0, 1, 5, 4],
        [3, 2, 6, 7],
    ];
    let corners = [
        Vec3::new(-1.0, -1.0, -1.0),
        Vec3::new(1.0, -1.0, -1.0),
        Vec3::new(1.0, 1.0, -1.0),
        Vec3::new(-1.0, 1.0, -1.0),
        Vec3::new(-1.0, -1.0, 1.0),
        Vec3::new(1.0, -1.0, 1.0),
        Vec3::new(1.0, 1.0, 1.0),
        Vec3::new(-1.0, 1.0, 1.0),
    ];

    for face in FACES {
        let a = center + corners[face[0]] * half_size;
        let b = center + corners[face[1]] * half_size;
        let c = center + corners[face[2]] * half_size;
        let d = center + corners[face[3]] * half_size;
        append_triangle(vertices, a, b, c, color);
        append_triangle(vertices, a, c, d, color);
    }
}

fn append_triangle(vertices: &mut Vec<GizmoVertex>, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
    vertices.extend([a, b, c].map(|position| GizmoVertex {
        position: position.to_array(),
        color,
    }));
}

fn perpendicular_basis(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let u = axis.cross(helper).normalize();
    let v = axis.cross(u).normalize();
    (u, v)
}
