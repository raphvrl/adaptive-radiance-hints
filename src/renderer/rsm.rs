use std::cmp::Reverse;

use anyhow::{Context, Result, ensure};
use glam::Mat4;

use crate::renderer::{frustum::Frustum, mesh::Aabb};

pub const GUTTER: u32 = 2;
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth16Unorm;

const ATLAS_WIDTH_STEP: u32 = 128;
const EMPTY_ATLAS_SIZE: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct RsmView {
    pub view_proj: Mat4,
    pub light_index: u32,
    pub resolution: u32,
    pub x: u32,
    pub y: u32,
    pub active: bool,
}

pub struct RsmAtlas {
    #[allow(dead_code)]
    normal_texture: wgpu::Texture,
    normal_view: wgpu::TextureView,

    #[allow(dead_code)]
    flux_texture: wgpu::Texture,
    flux_view: wgpu::TextureView,

    #[allow(dead_code)]
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,

    width: u32,
    height: u32,
}

impl RsmAtlas {
    pub fn new(device: &wgpu::Device) -> Self {
        Self::create(device, EMPTY_ATLAS_SIZE, EMPTY_ATLAS_SIZE)
    }

    pub fn ensure_capacity(&mut self, device: &wgpu::Device, views: &mut [RsmView]) -> Result<()> {
        let max_size = device.limits().max_texture_dimension_2d;
        let active_indices = views
            .iter()
            .enumerate()
            .filter_map(|(index, view)| view.active.then_some(index))
            .collect::<Vec<_>>();

        for view in views.iter_mut().filter(|view| !view.active) {
            view.x = 0;
            view.y = 0;
        }

        if active_indices.is_empty() {
            return Ok(());
        }

        let active_views = active_indices
            .iter()
            .map(|&index| views[index])
            .collect::<Vec<_>>();

        if let Some((_, placements)) = try_pack_active_views(&active_views, self.width, self.height)
        {
            for (&index, (x, y)) in active_indices.iter().zip(placements) {
                views[index].x = x;
                views[index].y = y;
            }

            return Ok(());
        }

        let (required_width, required_height, placements) = pack_views(&active_views, max_size)?;
        for (&index, (x, y)) in active_indices.iter().zip(placements) {
            views[index].x = x;
            views[index].y = y;
        }

        let width = self.width.max(required_width);
        let height = self.height.max(required_height);
        if width != self.width || height != self.height {
            *self = Self::create(device, width, height);
        }

        Ok(())
    }

    pub fn normal_view(&self) -> &wgpu::TextureView {
        &self.normal_view
    }

    pub fn flux_view(&self) -> &wgpu::TextureView {
        &self.flux_view
    }

    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    fn create(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let (normal_texture, normal_view) =
            create_color_target(device, "RSM normal", width, height);
        let (flux_texture, flux_view) = create_color_target(device, "RSM flux", width, height);

        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RSM depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let depth_view = depth_texture.create_view(&Default::default());

        Self {
            normal_texture,
            normal_view,
            flux_texture,
            flux_view,
            depth_texture,
            depth_view,
            width,
            height,
        }
    }
}

pub fn cull_empty_views(views: &mut [RsmView], world_bounds: &[Aabb]) -> Result<()> {
    for view in views {
        view.active = false;
        if world_bounds.is_empty() {
            continue;
        }

        let frustum = Frustum::from_view_proj(view.view_proj)?;
        view.active = world_bounds
            .iter()
            .copied()
            .any(|bounds| frustum.intersects_aabb(bounds));
    }

    Ok(())
}

fn pack_views(views: &[RsmView], max_size: u32) -> Result<(u32, u32, Vec<(u32, u32)>)> {
    if views.is_empty() {
        return Ok((EMPTY_ATLAS_SIZE, EMPTY_ATLAS_SIZE, Vec::new()));
    }

    let tile_sizes = views
        .iter()
        .map(|view| {
            view.resolution
                .checked_add(2 * GUTTER)
                .context("RSM tile size overflow")
        })
        .collect::<Result<Vec<_>>>()?;
    let mut order = (0..views.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| (Reverse(tile_sizes[index]), index));

    let largest_tile = tile_sizes[order[0]];
    ensure!(
        largest_tile <= max_size,
        "an RSM view exceeds the GPU's maximum 2D texture size of {max_size}"
    );

    let mut candidate_width = largest_tile;
    let mut best: Option<((u64, bool, u32, u32), u32, u32, Vec<(u32, u32)>)> = None;

    loop {
        if let Some((width, height, placements)) =
            try_pack_views(&tile_sizes, &order, candidate_width, max_size)
        {
            let area = u64::from(width) * u64::from(height);
            let score = (area, width < height, width.abs_diff(height), width);

            if best
                .as_ref()
                .is_none_or(|(best_score, _, _, _)| score < *best_score)
            {
                best = Some((score, width, height, placements));
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
        best.context("RSM views do not fit within the GPU's maximum 2D texture dimensions")?;

    Ok((width, height, placements))
}

fn try_pack_active_views(
    views: &[RsmView],
    atlas_width: u32,
    atlas_height: u32,
) -> Option<(u32, Vec<(u32, u32)>)> {
    let tile_sizes = views
        .iter()
        .map(|view| view.resolution.checked_add(2 * GUTTER))
        .collect::<Option<Vec<_>>>()?;
    let mut order = (0..views.len()).collect::<Vec<_>>();
    order.sort_by_key(|&index| (Reverse(tile_sizes[index]), index));

    let mut placements = vec![(0, 0); views.len()];
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut row_height = 0_u32;
    let mut packed_width = 0_u32;

    for index in order {
        let tile_size = tile_sizes[index];
        if tile_size > atlas_width {
            return None;
        }

        if x.checked_add(tile_size)? > atlas_width {
            y = y.checked_add(row_height)?;
            x = 0;
            row_height = 0;
        }

        if y.checked_add(tile_size)? > atlas_height {
            return None;
        }

        placements[index] = (x, y);
        packed_width = packed_width.max(x.checked_add(tile_size)?);
        x = x.checked_add(tile_size)?;
        row_height = row_height.max(tile_size);
    }

    Some((packed_width.max(EMPTY_ATLAS_SIZE), placements))
}

fn try_pack_views(
    tile_sizes: &[u32],
    order: &[usize],
    atlas_width: u32,
    max_height: u32,
) -> Option<(u32, u32, Vec<(u32, u32)>)> {
    let mut placements = vec![(0, 0); tile_sizes.len()];
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut row_height = 0_u32;
    let mut packed_width = 0_u32;

    for &index in order {
        let tile_size = tile_sizes[index];
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
        packed_width = packed_width.max(x.checked_add(tile_size)?);
        x = x.checked_add(tile_size)?;
        row_height = row_height.max(tile_size);
    }

    let packed_height = y.checked_add(row_height)?;

    Some((packed_width, packed_height, placements))
}

fn create_color_target(
    device: &wgpu::Device,
    label: &'static str,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    (texture, view)
}
