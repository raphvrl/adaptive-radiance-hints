use std::{cmp::Reverse, collections::HashMap, num::NonZeroU64};

use anyhow::{Context, Result, ensure};
use encase::{ArrayLength, ShaderType, StorageBuffer, UniformBuffer};
use glam::{
    Mat4, Vec3, Vec4,
    camera::rh::{proj::directx, view},
};
use hecs::Entity;
use wesl_core::include_wesl;
use wgpu::util::DeviceExt;

use crate::{
    renderer::{
        assets::MeshAssets,
        frustum::Frustum,
        instances::{InstanceBuffer, InstanceRaw, MeshBatch},
        mesh::{Aabb, MeshVertex},
    },
    scene::{
        PointLightInstance, SpotLightInstance,
        components::{POINT_SHADOW_FACE_COUNT, SHADOW_RESOLUTION_OPTIONS},
    },
};

const EMPTY_ATLAS_SIZE: u32 = 1;
const ATLAS_WIDTH_STEP: u32 = 128;
const TILE_GUTTER: u32 = 2;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;
const MIN_NEAR_PLANE: f32 = 0.001;
const SHADOW_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] =
    wgpu::vertex_attr_array![0 => Float32x3];
const SHADOW_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4
];
const SHADOW_VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<MeshVertex>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &SHADOW_VERTEX_ATTRIBUTES,
};
const SHADOW_INSTANCE_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Instance,
    attributes: &SHADOW_INSTANCE_ATTRIBUTES,
};

#[derive(Clone, Copy)]
pub struct ShadowView {
    pub tile_index: u32,
    pub view_proj: Mat4,
    camera_visible: bool,
    scene_visible: bool,
    enabled: bool,
    resolution: u32,
    tile_size: u32,
    x: u32,
    y: u32,
    uniform_offset: u32,
}

impl ShadowView {
    fn needs_atlas_tile(&self) -> bool {
        self.enabled && self.camera_visible && self.scene_visible
    }
}

pub struct ShadowAtlas {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    shadow_views: wgpu::Buffer,
    shadow_views_capacity: u64,
    width: u32,
    height: u32,
}

pub struct ShadowPlan {
    pub views: Vec<ShadowView>,
    point_tiles: HashMap<Entity, u32>,
    spot_tiles: HashMap<Entity, u32>,
}

impl ShadowPlan {
    pub fn point_tile(&self, entity: Entity) -> Option<u32> {
        self.point_tiles.get(&entity).copied()
    }

    pub fn spot_tile(&self, entity: Entity) -> Option<u32> {
        self.spot_tiles.get(&entity).copied()
    }

    pub fn point_face_camera_visibility(
        &self,
        entity: Entity,
    ) -> Option<[bool; POINT_SHADOW_FACE_COUNT]> {
        let first_view = usize::try_from(*self.point_tiles.get(&entity)?).ok()?;
        let end_view = first_view.checked_add(POINT_SHADOW_FACE_COUNT)?;
        let views = self.views.get(first_view..end_view)?;

        Some(std::array::from_fn(|index| views[index].camera_visible))
    }

    pub fn spot_camera_visibility(&self, entity: Entity) -> Option<bool> {
        let view_index = usize::try_from(*self.spot_tiles.get(&entity)?).ok()?;

        self.views.get(view_index).map(|view| view.camera_visible)
    }
}

#[derive(ShaderType)]
struct GpuShadowView {
    view_proj: Mat4,
    atlas_uv: Vec4,
    has_atlas_tile: u32,
}

#[derive(ShaderType)]
struct ShadowViews {
    length: ArrayLength,
    #[shader(size(runtime))]
    views: Vec<GpuShadowView>,
}

impl ShadowAtlas {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow atlas bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let size = EMPTY_ATLAS_SIZE;
        let (texture, view) = Self::create_texture(device, size, size);
        let shadow_views_contents = Self::serialize_views(size, size, &[])?;
        let shadow_views_capacity = u64::try_from(shadow_views_contents.len())
            .context("shadow view buffer size overflow")?;
        let shadow_views = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shadow view storage buffer"),
            contents: &shadow_views_contents,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = Self::create_bind_group(device, &layout, &shadow_views, &view, &sampler);

        Ok(Self {
            texture,
            view,
            sampler,
            layout,
            bind_group,
            shadow_views,
            shadow_views_capacity,
            width: size,
            height: size,
        })
    }

    pub fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        views: &mut [ShadowView],
    ) -> Result<()> {
        if let Some((_, placements)) = try_pack_views(views, self.width, self.height) {
            for (view, (x, y)) in views.iter_mut().zip(placements) {
                view.x = x;
                view.y = y;
            }

            return Ok(());
        }

        let (required_width, required_height, placements) = pack_views(
            views,
            self.width,
            self.height,
            device.limits().max_texture_dimension_2d,
        )?;

        for (view, (x, y)) in views.iter_mut().zip(placements) {
            view.x = x;
            view.y = y;
        }

        let width = self.width.max(required_width);
        let height = self.height.max(required_height);

        let (texture, view) = Self::create_texture(device, width, height);
        let bind_group = Self::create_bind_group(
            device,
            &self.layout,
            &self.shadow_views,
            &view,
            &self.sampler,
        );

        self.texture = texture;
        self.view = view;
        self.bind_group = bind_group;
        self.width = width;
        self.height = height;

        Ok(())
    }

    pub fn write_views(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        views: &[ShadowView],
    ) -> Result<()> {
        let contents = Self::serialize_views(self.width, self.height, views)?;
        let required = u64::try_from(contents.len()).context("shadow view buffer size overflow")?;
        self.ensure_view_capacity(device, required)?;
        queue.write_buffer(&self.shadow_views, 0, &contents);
        Ok(())
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    fn ensure_view_capacity(&mut self, device: &wgpu::Device, required: u64) -> Result<()> {
        if required <= self.shadow_views_capacity {
            return Ok(());
        }

        let capacity = required
            .checked_next_power_of_two()
            .context("shadow view buffer capacity overflow")?;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow view storage buffer"),
            size: capacity,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group =
            Self::create_bind_group(device, &self.layout, &buffer, &self.view, &self.sampler);

        self.shadow_views = buffer;
        self.bind_group = bind_group;
        self.shadow_views_capacity = capacity;

        Ok(())
    }

    fn serialize_views(width: u32, height: u32, views: &[ShadowView]) -> Result<Vec<u8>> {
        let atlas_width = width as f32;
        let atlas_height = height as f32;
        let gpu_views = views
            .iter()
            .map(|view| {
                let map_size = view.resolution as f32;

                GpuShadowView {
                    view_proj: view.view_proj,
                    atlas_uv: Vec4::new(
                        map_size / atlas_width,
                        map_size / atlas_height,
                        (view.x + TILE_GUTTER) as f32 / atlas_width,
                        (view.y + TILE_GUTTER) as f32 / atlas_height,
                    ),
                    has_atlas_tile: if view.needs_atlas_tile() { 1 } else { 0 },
                }
            })
            .collect();
        let shadow_views = ShadowViews {
            length: ArrayLength,
            views: gpu_views,
        };

        Ok(StorageBuffer::<Vec<u8>>::content_of(&shadow_views)?)
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Shadow atlas texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (texture, view)
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        shadow_views: &wgpu::Buffer,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow atlas bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shadow_views.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

pub struct ShadowPass {
    pipeline: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    matrix_bind_group: wgpu::BindGroup,
    matrix_buffer: wgpu::Buffer,
    matrix_size: NonZeroU64,
    matrix_stride: u64,
    matrix_capacity: u32,
}

#[derive(ShaderType)]
struct ShadowFrameUniform {
    view_proj: Mat4,
}

impl ShadowPass {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let initial_contents = Self::serialize_matrix(Mat4::IDENTITY)?;
        let matrix_size = NonZeroU64::new(
            u64::try_from(initial_contents.len()).context("shadow matrix size overflow")?,
        )
        .context("shadow matrix uniform cannot be empty")?;
        let matrix_stride = aligned_stride(
            matrix_size.get(),
            u64::from(device.limits().min_uniform_buffer_offset_alignment),
        )?;

        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Shadow frame bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(matrix_size),
                },
                count: None,
            }],
        });

        let matrix_capacity = 1;
        let matrix_buffer = Self::create_matrix_buffer(device, matrix_stride, matrix_capacity)?;
        let matrix_bind_group =
            Self::create_matrix_bind_group(device, &frame_layout, &matrix_buffer, matrix_size);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("shadow").into()),
        });

        let bind_group_layouts = [Some(&frame_layout)];
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Shadow pipeline layout"),
            bind_group_layouts: &bind_group_layouts,
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Shadow pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                compilation_options: Default::default(),
                buffers: &[Some(SHADOW_VERTEX_LAYOUT), Some(SHADOW_INSTANCE_LAYOUT)],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });

        Ok(Self {
            pipeline,
            frame_layout,
            matrix_bind_group,
            matrix_buffer,
            matrix_size,
            matrix_stride,
            matrix_capacity,
        })
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        atlas: &wgpu::TextureView,
        assets: &MeshAssets,
        instances: &InstanceBuffer,
        batches: &[MeshBatch],
        views: &mut [ShadowView],
    ) -> Result<()> {
        self.write_matrices(device, queue, views)?;

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Shadow atlas pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: atlas,
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
        pass.set_vertex_buffer(1, instances.buffer().slice(..));

        for view in views {
            if !view.needs_atlas_tile() {
                continue;
            }

            let map_size = view.resolution;

            pass.set_viewport(
                (view.x + TILE_GUTTER) as f32,
                (view.y + TILE_GUTTER) as f32,
                map_size as f32,
                map_size as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(view.x, view.y, view.tile_size, view.tile_size);
            pass.set_bind_group(0, &self.matrix_bind_group, &[view.uniform_offset]);

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

    fn write_matrices(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        views: &mut [ShadowView],
    ) -> Result<()> {
        let required = u32::try_from(views.len()).context("too many shadow views")?;
        self.ensure_matrix_capacity(device, required)?;

        let write_size = self
            .matrix_stride
            .checked_mul(u64::from(required))
            .context("shadow matrix write size overflow")?;
        let write_size =
            usize::try_from(write_size).context("shadow matrix write size overflow")?;
        let mut contents = vec![0; write_size];

        for view in views {
            let matrix_contents = Self::serialize_matrix(view.view_proj)?;
            ensure!(
                u64::try_from(matrix_contents.len()).context("shadow matrix size overflow")?
                    == self.matrix_size.get(),
                "serialized shadow matrix has an unexpected size"
            );

            let offset = u64::from(view.tile_index)
                .checked_mul(self.matrix_stride)
                .context("shadow matrix offset overflow")?;
            let end = offset
                .checked_add(self.matrix_size.get())
                .context("shadow matrix buffer range overflow")?;

            ensure!(
                end <= u64::try_from(contents.len())
                    .context("shadow matrix write size overflow")?,
                "shadow tile index {} exceeds the matrix buffer capacity",
                view.tile_index
            );

            let offset = usize::try_from(offset).context("shadow matrix offset overflow")?;
            let end = usize::try_from(end).context("shadow matrix end offset overflow")?;
            contents[offset..end].copy_from_slice(&matrix_contents);
            view.uniform_offset =
                u32::try_from(offset).context("shadow matrix dynamic offset overflow")?;
        }

        if !contents.is_empty() {
            queue.write_buffer(&self.matrix_buffer, 0, &contents);
        }

        Ok(())
    }

    fn ensure_matrix_capacity(&mut self, device: &wgpu::Device, required: u32) -> Result<()> {
        if required <= self.matrix_capacity {
            return Ok(());
        }

        let capacity = required
            .checked_next_power_of_two()
            .context("shadow matrix buffer capacity overflow")?;
        let buffer = Self::create_matrix_buffer(device, self.matrix_stride, capacity)?;
        let bind_group =
            Self::create_matrix_bind_group(device, &self.frame_layout, &buffer, self.matrix_size);

        self.matrix_buffer = buffer;
        self.matrix_bind_group = bind_group;
        self.matrix_capacity = capacity;

        Ok(())
    }

    fn serialize_matrix(view_proj: Mat4) -> Result<Vec<u8>> {
        Ok(UniformBuffer::<Vec<u8>>::content_of(&ShadowFrameUniform {
            view_proj,
        })?)
    }

    fn create_matrix_buffer(
        device: &wgpu::Device,
        stride: u64,
        capacity: u32,
    ) -> Result<wgpu::Buffer> {
        let size = stride
            .checked_mul(u64::from(capacity))
            .context("shadow matrix buffer size overflow")?;

        Ok(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Shadow matrix uniform buffer"),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }))
    }

    fn create_matrix_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        matrix_size: NonZeroU64,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Shadow matrix bind group"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: Some(matrix_size),
                }),
            }],
        })
    }
}

pub fn build_plan(
    point_lights: &[PointLightInstance],
    spot_lights: &[SpotLightInstance],
    camera_view_proj: Mat4,
    world_bounds: &[Aabb],
) -> Result<ShadowPlan> {
    let camera_frustum = Frustum::from_view_proj(camera_view_proj)?;
    let capacity = point_lights
        .len()
        .checked_mul(POINT_SHADOW_FACE_COUNT)
        .and_then(|count| count.checked_add(spot_lights.len()))
        .context("shadow view count overflow")?;
    let mut views = Vec::with_capacity(capacity);
    let mut point_tiles = HashMap::with_capacity(point_lights.len());
    let mut spot_tiles = HashMap::with_capacity(spot_lights.len());
    let mut tile_index = 0;

    let point_faces = [
        (Vec3::X, -Vec3::Y),
        (-Vec3::X, -Vec3::Y),
        (Vec3::Y, Vec3::Z),
        (-Vec3::Y, -Vec3::Z),
        (Vec3::Z, -Vec3::Y),
        (-Vec3::Z, -Vec3::Y),
    ];

    for light in point_lights {
        ensure!(
            light.position.is_finite(),
            "point light position is invalid"
        );
        ensure!(
            SHADOW_RESOLUTION_OPTIONS.contains(&light.shadow_resolution),
            "point light has an unsupported shadow resolution {}",
            light.shadow_resolution
        );
        let near = near_plane(light.range)?;
        let projection = directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, near, light.range);
        point_tiles.insert(light.entity, tile_index);

        for (face_index, (direction, up)) in point_faces.into_iter().enumerate() {
            let view_proj = projection * view::look_to_mat4(light.position, direction, up);
            push_view(
                &mut views,
                view_proj,
                light.shadow_resolution,
                &camera_frustum,
                world_bounds,
                light.shadow_faces_enabled[face_index],
                &mut tile_index,
            )?;
        }
    }

    for light in spot_lights {
        ensure!(light.position.is_finite(), "spot light position is invalid");
        ensure!(
            SHADOW_RESOLUTION_OPTIONS.contains(&light.shadow_resolution),
            "spot light has an unsupported shadow resolution {}",
            light.shadow_resolution
        );
        let direction_length_squared = light.direction.length_squared();
        ensure!(
            light.direction.is_finite()
                && direction_length_squared.is_finite()
                && direction_length_squared > f32::EPSILON,
            "spot light direction is invalid"
        );

        let direction = light.direction.normalize();
        let up = if direction.dot(Vec3::Y).abs() > 0.99 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let fov = light.outer_angle * 2.0;
        ensure!(
            fov.is_finite() && fov > 0.0 && fov < std::f32::consts::PI,
            "spot light has an invalid shadow field of view"
        );

        let near = near_plane(light.range)?;
        let projection = directx::perspective(fov, 1.0, near, light.range);
        let view_proj = projection * view::look_to_mat4(light.position, direction, up);

        spot_tiles.insert(light.entity, tile_index);
        push_view(
            &mut views,
            view_proj,
            light.shadow_resolution,
            &camera_frustum,
            world_bounds,
            true,
            &mut tile_index,
        )?;
    }

    Ok(ShadowPlan {
        views,
        point_tiles,
        spot_tiles,
    })
}

fn push_view(
    views: &mut Vec<ShadowView>,
    view_proj: Mat4,
    resolution: u32,
    camera_frustum: &Frustum,
    world_bounds: &[Aabb],
    enabled: bool,
    tile_index: &mut u32,
) -> Result<()> {
    let view_frustum = Frustum::from_view_proj(view_proj)?;
    let camera_visible = enabled && camera_frustum.intersects(view_frustum);
    let scene_visible = camera_visible
        && world_bounds
            .iter()
            .copied()
            .any(|bounds| view_frustum.intersects_aabb(bounds));

    views.push(ShadowView {
        tile_index: *tile_index,
        view_proj,
        camera_visible,
        scene_visible,
        enabled,
        resolution,
        tile_size: resolution + 2 * TILE_GUTTER,
        x: 0,
        y: 0,
        uniform_offset: 0,
    });

    *tile_index = (*tile_index)
        .checked_add(1)
        .context("shadow tile index overflow")?;

    Ok(())
}

fn pack_views(
    views: &[ShadowView],
    current_width: u32,
    current_height: u32,
    max_size: u32,
) -> Result<(u32, u32, Vec<(u32, u32)>)> {
    ensure!(
        current_width > 0
            && current_height > 0
            && current_width <= max_size
            && current_height <= max_size,
        "shadow atlas size exceeds the GPU's maximum 2D texture size of {max_size}"
    );

    let Some(largest_tile) = views
        .iter()
        .filter(|view| view.needs_atlas_tile())
        .map(|view| view.tile_size)
        .max()
    else {
        return Ok((
            EMPTY_ATLAS_SIZE,
            EMPTY_ATLAS_SIZE,
            vec![(0, 0); views.len()],
        ));
    };

    ensure!(
        largest_tile <= max_size,
        "a shadow map exceeds the GPU's maximum 2D texture size of {max_size}"
    );

    if let Some((required_height, placements)) = try_pack_views(views, current_width, max_size) {
        if required_height <= current_height {
            let required_width = packed_width(views, &placements)?;
            return Ok((required_width, required_height, placements));
        }
    }

    let mut candidate_width = current_width.max(largest_tile);

    let mut best: Option<(u64, u32, u32, Vec<(u32, u32)>)> = None;
    loop {
        if let Some((required_height, placements)) =
            try_pack_views(views, candidate_width, max_size)
        {
            let width = packed_width(views, &placements)?;
            let area = u64::from(width) * u64::from(required_height);
            let replace = best
                .as_ref()
                .map_or(true, |(best_area, _, _, _)| area < *best_area);

            if replace {
                best = Some((area, width, required_height, placements));
            }
        }

        if candidate_width == max_size {
            break;
        }

        candidate_width = candidate_width
            .saturating_add(ATLAS_WIDTH_STEP)
            .min(max_size);
    }

    let (_, width, height, placements) =
        best.context("shadow maps do not fit within the GPU's maximum 2D texture dimensions")?;

    Ok((width, height, placements))
}

fn packed_width(views: &[ShadowView], placements: &[(u32, u32)]) -> Result<u32> {
    let width = views
        .iter()
        .zip(placements)
        .filter(|(view, _)| view.needs_atlas_tile())
        .try_fold(0, |width, (view, (x, _))| {
            let right = x
                .checked_add(view.tile_size)
                .context("shadow atlas width overflow")?;

            Ok::<_, anyhow::Error>(width.max(right))
        })?;

    Ok(width.max(EMPTY_ATLAS_SIZE))
}

fn try_pack_views(
    views: &[ShadowView],
    atlas_width: u32,
    max_height: u32,
) -> Option<(u32, Vec<(u32, u32)>)> {
    let mut order = views
        .iter()
        .enumerate()
        .filter_map(|(index, view)| view.needs_atlas_tile().then_some(index))
        .collect::<Vec<_>>();
    order.sort_by_key(|&index| (Reverse(views[index].tile_size), index));

    let mut placements = vec![(0, 0); views.len()];
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut row_height: u32 = 0;

    for index in order {
        let tile_size = views[index].tile_size;
        if tile_size > atlas_width {
            return None;
        }

        if x.checked_add(tile_size)? > atlas_width {
            y = y.checked_add(row_height)?;
            x = 0;
            row_height = 0;
        }

        if y.checked_add(tile_size)? > max_height {
            return None;
        }

        placements[index] = (x, y);
        x = x.checked_add(tile_size)?;
        row_height = row_height.max(tile_size);
    }

    Some((y.checked_add(row_height)?, placements))
}

fn near_plane(range: f32) -> Result<f32> {
    ensure!(range.is_finite() && range > 0.0, "light range is invalid");

    let near = (range * 0.05).max(MIN_NEAR_PLANE).min(range * 0.5);
    ensure!(
        near > 0.0 && near < range,
        "light range is too small for shadows"
    );

    Ok(near)
}

fn aligned_stride(size: u64, alignment: u64) -> Result<u64> {
    let alignment = alignment.max(1);
    let padding = (alignment - size % alignment) % alignment;

    size.checked_add(padding)
        .context("shadow matrix stride overflow")
}
