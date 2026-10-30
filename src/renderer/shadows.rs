use std::{
    cmp::Reverse,
    collections::HashMap,
    num::NonZeroU64,
};

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
        instances::{InstanceBuffer, InstanceRaw, MeshBatch},
        mesh::MeshVertex,
    },
    scene::{
        PointLightInstance, ShadowCamera, SpotLightInstance, components::POINT_SHADOW_FACE_COUNT,
    },
};

const INITIAL_ATLAS_SIZE: u32 = 512;
const MIN_SHADOW_RESOLUTION: u32 = 128;
const MAX_SHADOW_RESOLUTION: u32 = 2048;
const MAX_SCREEN_PIXELS_PER_TEXEL: f32 = 1.0;
const TILE_GUTTER: u32 = 2;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MIN_NEAR_PLANE: f32 = 0.001;
const MIN_CAMERA_DISTANCE: f32 = 0.1;
const FRUSTUM_INTERSECTION_EPSILON: f32 = 0.001;
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
struct ClipFrustum {
    planes: [Vec4; 6],
    corners: [Vec3; 8],
}

impl ClipFrustum {
    fn from_view_proj(view_proj: Mat4) -> Result<Self> {
        ensure!(view_proj.is_finite(), "frustum view-projection matrix is invalid");

        let inverse = view_proj.inverse();
        ensure!(inverse.is_finite(), "frustum view-projection matrix is singular");

        let corners = [
            Self::unproject_corner(inverse, Vec4::new(-1.0, -1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, -1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, 1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, 1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, -1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, -1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, 1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, 1.0, 1.0, 1.0))?,
        ];

        let columns = view_proj.to_cols_array_2d();
        let row = |index| {
            Vec4::new(
                columns[0][index],
                columns[1][index],
                columns[2][index],
                columns[3][index],
            )
        };
        let row_0 = row(0);
        let row_1 = row(1);
        let row_2 = row(2);
        let row_3 = row(3);
        let planes = [
            Self::normalize_plane(row_3 + row_0)?,
            Self::normalize_plane(row_3 - row_0)?,
            Self::normalize_plane(row_3 + row_1)?,
            Self::normalize_plane(row_3 - row_1)?,
            Self::normalize_plane(row_2)?,
            Self::normalize_plane(row_3 - row_2)?,
        ];

        Ok(Self { planes, corners })
    }

    fn intersects(self, other: Self) -> bool {
        !self.separates(other) && !other.separates(self)
    }

    fn separates(self, other: Self) -> bool {
        // Face-plane checks are conservative: they may keep an unnecessary view, but never cull a needed one.
        self.planes.iter().any(|plane| {
            other
                .corners
                .iter()
                .all(|corner| plane.dot(corner.extend(1.0)) < -FRUSTUM_INTERSECTION_EPSILON)
        })
    }

    fn unproject_corner(inverse: Mat4, clip_corner: Vec4) -> Result<Vec3> {
        let world_corner = inverse * clip_corner;
        ensure!(
            world_corner.is_finite() && world_corner.w.abs() > f32::EPSILON,
            "frustum corner is invalid"
        );

        let world_corner = world_corner.truncate() / world_corner.w;
        ensure!(world_corner.is_finite(), "frustum corner is invalid");

        Ok(world_corner)
    }

    fn normalize_plane(plane: Vec4) -> Result<Vec4> {
        let normal_length = plane.truncate().length();
        ensure!(
            normal_length.is_finite() && normal_length > f32::EPSILON,
            "frustum plane is invalid"
        );

        Ok(plane / normal_length)
    }
}

#[derive(Clone, Copy)]
pub struct ShadowView {
    pub tile_index: u32,
    pub view_proj: Mat4,
    camera_visible: bool,
    enabled: bool,
    key: ShadowKey,
    resolution: u32,
    tile_size: u32,
    required_resolution: f32,
    x: u32,
    y: u32,
    uniform_offset: u32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ShadowKey {
    Point(Entity),
    Spot(Entity),
}

#[derive(Clone, Copy)]
struct ShadowResolutionDemand {
    resolution: u32,
    tile_size: u32,
    required: f32,
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
    resolution_cache: HashMap<ShadowKey, u32>,
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

        self.views
            .get(view_index)
            .map(|view| view.camera_visible)
    }
}

#[derive(ShaderType)]
struct GpuShadowView {
    view_proj: Mat4,
    atlas_uv: Vec4,
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
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let size = INITIAL_ATLAS_SIZE.min(device.limits().max_texture_dimension_2d);
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
            resolution_cache: HashMap::new(),
        })
    }

    pub fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        views: &mut [ShadowView],
    ) -> Result<()> {
        self.apply_resolution_hysteresis(views);

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

        if required_width == self.width && required_height == self.height {
            return Ok(());
        }

        let (texture, view) = Self::create_texture(device, required_width, required_height);
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
        self.width = required_width;
        self.height = required_height;

        Ok(())
    }

    fn apply_resolution_hysteresis(&mut self, views: &mut [ShadowView]) {
        let mut resolution_cache = HashMap::with_capacity(self.resolution_cache.len());

        for view in views {
            let resolution = match self.resolution_cache.get(&view.key).copied() {
                None => view.resolution,
                Some(current)
                    if view.resolution < current
                        && (view.resolution == MIN_SHADOW_RESOLUTION
                            || view.required_resolution <= view.resolution as f32 * 0.9) =>
                {
                    view.resolution
                }
                Some(current)
                    if view.resolution > current
                        && view.required_resolution >= current as f32 * 1.1 =>
                {
                    view.resolution
                }
                Some(current) => current,
            };

            view.resolution = resolution;
            view.tile_size = resolution + 2 * TILE_GUTTER;
            resolution_cache.insert(view.key, resolution);
        }

        self.resolution_cache = resolution_cache;
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

    pub fn resolution(&self, entity: Entity, is_spot: bool) -> Option<u32> {
        let key = if is_spot {
            ShadowKey::Spot(entity)
        } else {
            ShadowKey::Point(entity)
        };

        self.resolution_cache.get(&key).copied()
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
                bias: Default::default(),
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
            if !view.enabled || !view.camera_visible {
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
    camera: ShadowCamera,
    camera_view_proj: Mat4,
) -> Result<ShadowPlan> {
    let camera_frustum = ClipFrustum::from_view_proj(camera_view_proj)?;
    let capacity = point_lights
        .len()
        .checked_mul(POINT_SHADOW_FACE_COUNT)
        .and_then(|count| count.checked_add(spot_lights.len()))
        .context("shadow view count overflow")?;
    let mut views = Vec::with_capacity(capacity);
    let mut point_tiles = HashMap::with_capacity(point_lights.len());
    let mut spot_tiles = HashMap::with_capacity(spot_lights.len());
    let mut tile_index = 0;

    // Keep this order aligned with the point light face settings and shader face selection.
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
        let near = near_plane(light.range)?;
        let projection = directx::perspective(std::f32::consts::FRAC_PI_2, 1.0, near, light.range);
        let demand = resolution_demand(
            camera,
            light.position,
            light.range,
            std::f32::consts::FRAC_PI_4,
        )?;
        let key = ShadowKey::Point(light.entity);
        point_tiles.insert(light.entity, tile_index);

        for (face_index, (direction, up)) in point_faces.into_iter().enumerate() {
            let view_proj = projection * view::look_to_mat4(light.position, direction, up);
            push_view(
                &mut views,
                view_proj,
                key,
                demand,
                &camera_frustum,
                light.shadow_faces_enabled[face_index],
                &mut tile_index,
            )?;
        }
    }

    for light in spot_lights {
        ensure!(light.position.is_finite(), "spot light position is invalid");
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
        let demand = resolution_demand(camera, light.position, light.range, light.outer_angle)?;

        spot_tiles.insert(light.entity, tile_index);
        push_view(
            &mut views,
            view_proj,
            ShadowKey::Spot(light.entity),
            demand,
            &camera_frustum,
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
    key: ShadowKey,
    demand: ShadowResolutionDemand,
    camera_frustum: &ClipFrustum,
    enabled: bool,
    tile_index: &mut u32,
) -> Result<()> {
    let light_frustum = ClipFrustum::from_view_proj(view_proj)?;
    views.push(ShadowView {
        tile_index: *tile_index,
        view_proj,
        camera_visible: camera_frustum.intersects(light_frustum),
        enabled,
        key,
        resolution: demand.resolution,
        tile_size: demand.tile_size,
        required_resolution: demand.required,
        x: 0,
        y: 0,
        uniform_offset: 0,
    });

    *tile_index = (*tile_index)
        .checked_add(1)
        .context("shadow tile index overflow")?;

    Ok(())
}

fn resolution_demand(
    camera: ShadowCamera,
    light_position: Vec3,
    light_range: f32,
    light_half_fov: f32,
) -> Result<ShadowResolutionDemand> {
    ensure!(
        camera.position.is_finite() && light_position.is_finite(),
        "shadow camera or light position is invalid"
    );
    ensure!(camera.viewport_height > 0, "shadow viewport height is zero");
    ensure!(
        camera.fovy.is_finite()
            && camera.fovy > 0.0
            && camera.fovy < std::f32::consts::PI,
        "shadow camera field of view is invalid"
    );
    ensure!(
        light_range.is_finite()
            && light_range > 0.0
            && light_half_fov.is_finite()
            && light_half_fov > 0.0
            && light_half_fov < std::f32::consts::FRAC_PI_2,
        "shadow light projection is invalid"
    );

    // Use distance to the light as a proxy until visible receiver bounds are available.
    let camera_distance = camera
        .position
        .distance(light_position)
        .max(MIN_CAMERA_DISTANCE);

    // Estimate the shadow frustum width at its far plane and target one texel per screen pixel.
    let map_span = 2.0 * light_range * light_half_fov.tan();
    let required = camera.viewport_height as f32 * map_span
        / (2.0
            * MAX_SCREEN_PIXELS_PER_TEXEL
            * camera_distance
            * (camera.fovy * 0.5).tan());

    ensure!(
        camera_distance.is_finite() && map_span.is_finite() && required.is_finite(),
        "shadow resolution calculation overflowed"
    );

    let required_tier = required
        .ceil()
        .clamp(
            MIN_SHADOW_RESOLUTION as f32,
            MAX_SHADOW_RESOLUTION as f32,
        ) as u32;
    let resolution = required_tier
        .next_power_of_two()
        .min(MAX_SHADOW_RESOLUTION);

    Ok(ShadowResolutionDemand {
        resolution,
        tile_size: resolution + 2 * TILE_GUTTER,
        required,
    })
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

    if let Some((required_height, placements)) = try_pack_views(views, current_width, max_size) {
        if required_height <= current_height {
            return Ok((current_width, current_height, placements));
        }
    }

    let largest_tile = views
        .iter()
        .map(|view| view.tile_size)
        .max()
        .unwrap_or(1);
    let mut candidate_width = current_width.max(largest_tile);
    ensure!(
        candidate_width <= max_size,
        "a shadow map exceeds the GPU's maximum 2D texture size of {max_size}"
    );

    let mut best: Option<(u64, u32, u32, Vec<(u32, u32)>)> = None;
    loop {
        if let Some((required_height, placements)) =
            try_pack_views(views, candidate_width, max_size)
        {
            let height = current_height.max(required_height);
            let area = u64::from(candidate_width) * u64::from(height);
            let replace = best
                .as_ref()
                .map_or(true, |(best_area, _, _, _)| area < *best_area);

            if replace {
                best = Some((area, candidate_width, height, placements));
            }
        }

        if candidate_width == max_size {
            break;
        }

        candidate_width = candidate_width.saturating_add(MIN_SHADOW_RESOLUTION).min(max_size);
    }

    let (_, width, height, placements) = best.context(
        "shadow maps do not fit within the GPU's maximum 2D texture dimensions",
    )?;

    Ok((width, height, placements))
}

fn try_pack_views(
    views: &[ShadowView],
    atlas_width: u32,
    max_height: u32,
) -> Option<(u32, Vec<(u32, u32)>)> {
    let mut order: Vec<usize> = (0..views.len()).collect();
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

    let near = (range * 0.01).max(MIN_NEAR_PLANE).min(range * 0.5);
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
