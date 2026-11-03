use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub const TILE_SIZE: u32 = 16;
const MAX_LIGHTS_PER_TILE: u32 = 64;

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileParams {
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    max_lights_per_tile: u32,
}

struct TileResources {
    width: u32,
    height: u32,
    tiles_x: u32,
    tiles_y: u32,
    max_lights_per_tile: u32,
    _data: wgpu::Buffer,
    _params: wgpu::Buffer,
    geometry_bind_group: wgpu::BindGroup,
    compute_bind_group: wgpu::BindGroup,
}

impl TileResources {
    fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        geometry_layout: &wgpu::BindGroupLayout,
        compute_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let tiles_x = width.div_ceil(TILE_SIZE);
        let tiles_y = height.div_ceil(TILE_SIZE);
        let tile_count = u64::from(tiles_x) * u64::from(tiles_y);
        let header_words = tile_count * 2;
        let max_storage_words = u64::from(device.limits().max_storage_buffer_binding_size) / 4;
        let max_lights_per_tile = (max_storage_words.saturating_sub(header_words) / tile_count)
            .min(u64::from(MAX_LIGHTS_PER_TILE))
            .max(1) as u32;
        let data_words = header_words + tile_count * u64::from(max_lights_per_tile);

        let data = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tile light data"),
            size: (data_words * std::mem::size_of::<u32>() as u64).max(4),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Tile light params"),
            contents: bytemuck::bytes_of(&TileParams {
                tiles_x,
                tiles_y,
                tile_size: TILE_SIZE,
                max_lights_per_tile,
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tile light geometry bind group"),
            layout: geometry_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: data.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Tile light compute bind group"),
            layout: compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: data.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: params.as_entire_binding(),
                },
            ],
        });

        Self {
            width,
            height,
            tiles_x,
            tiles_y,
            max_lights_per_tile,
            _data: data,
            _params: params,
            geometry_bind_group,
            compute_bind_group,
        }
    }
}

pub struct TileLists {
    geometry_layout: wgpu::BindGroupLayout,
    compute_layout: wgpu::BindGroupLayout,
    resources: TileResources,
}

impl TileLists {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let geometry_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tile light geometry layout"),
            entries: &[
                storage_layout_entry(0, wgpu::ShaderStages::FRAGMENT, true),
                uniform_layout_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Tile light compute layout"),
            entries: &[
                storage_layout_entry(0, wgpu::ShaderStages::COMPUTE, false),
                uniform_layout_entry(1, wgpu::ShaderStages::COMPUTE),
            ],
        });
        let resources =
            TileResources::new(device, width, height, &geometry_layout, &compute_layout);

        Self {
            geometry_layout,
            compute_layout,
            resources,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.resources.width && height == self.resources.height {
            return;
        }

        self.resources = TileResources::new(
            device,
            width,
            height,
            &self.geometry_layout,
            &self.compute_layout,
        );
    }

    pub fn geometry_layout(&self) -> &wgpu::BindGroupLayout {
        &self.geometry_layout
    }

    pub fn geometry_bind_group(&self) -> &wgpu::BindGroup {
        &self.resources.geometry_bind_group
    }

    pub fn compute_layout(&self) -> &wgpu::BindGroupLayout {
        &self.compute_layout
    }

    pub fn compute_bind_group(&self) -> &wgpu::BindGroup {
        &self.resources.compute_bind_group
    }

    pub fn width(&self) -> u32 {
        self.resources.width
    }

    pub fn height(&self) -> u32 {
        self.resources.height
    }

    pub fn tiles_x(&self) -> u32 {
        self.resources.tiles_x
    }

    pub fn tiles_y(&self) -> u32 {
        self.resources.tiles_y
    }

    pub fn max_lights_per_tile(&self) -> u32 {
        self.resources.max_lights_per_tile
    }
}

fn storage_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    read_only: bool,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
