use anyhow::{Context, Result, ensure};
use glam::Vec3;

use crate::scene::MeshInstance;

use super::mesh::{Aabb, MeshData};

pub const VOXEL_SIZE_METERS: f32 = 0.125;
pub const VOXELS_PER_CHUNK: usize = 8;
pub const CHUNK_SIZE_METERS: f32 = VOXEL_SIZE_METERS * VOXELS_PER_CHUNK as f32;
pub const CHUNKS_PER_AXIS: usize = 8;
pub const VOXELS_PER_AXIS: usize = VOXELS_PER_CHUNK * CHUNKS_PER_AXIS;
pub const CHUNK_COUNT: usize = CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * CHUNKS_PER_AXIS;
pub const REGION_CHUNK_OFFSET: [f32; 3] = [-0.5, -0.125, -0.5];
pub const REGION_ORIGIN: [f32; 3] = [
    -CHUNK_SIZE_METERS + REGION_CHUNK_OFFSET[0],
    REGION_CHUNK_OFFSET[1],
    -CHUNK_SIZE_METERS + REGION_CHUNK_OFFSET[2],
];

pub const WORDS_PER_CHUNK: usize = VOXELS_PER_CHUNK * VOXELS_PER_CHUNK * VOXELS_PER_CHUNK / 32;
pub const DIRTY_WORDS: usize = CHUNK_COUNT / 32;
const BLOCKS_PER_AXIS: usize = VOXELS_PER_CHUNK / 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_stores_each_block_in_separate_words() {
        let mut chunk = VoxelChunk::default();

        for z in 0..2 {
            for y in 0..2 {
                for x in 0..2 {
                    chunk.mark_occupied(x * 4, y * 4, z * 4);
                }
            }
        }

        assert_eq!(chunk.occupancy_words(), &[1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0]);
    }

    #[test]
    fn chunk_stores_last_voxel_in_last_word() {
        let mut chunk = VoxelChunk::default();
        chunk.mark_occupied(7, 7, 7);

        assert_eq!(chunk.occupancy_words()[15], 1 << 31);
    }
}

pub fn chunk_mask_for_bounds(bounds: Aabb, origin: Vec3, voxel_size: f32) -> [u32; DIRTY_WORDS] {
    let Some((min_x, max_x)) =
        candidate_range_with_size(bounds.min.x, bounds.max.x, origin.x, voxel_size)
    else {
        return [0; DIRTY_WORDS];
    };
    let Some((min_y, max_y)) =
        candidate_range_with_size(bounds.min.y, bounds.max.y, origin.y, voxel_size)
    else {
        return [0; DIRTY_WORDS];
    };
    let Some((min_z, max_z)) =
        candidate_range_with_size(bounds.min.z, bounds.max.z, origin.z, voxel_size)
    else {
        return [0; DIRTY_WORDS];
    };

    let mut mask = [0; DIRTY_WORDS];

    for z in min_z / VOXELS_PER_CHUNK..=max_z / VOXELS_PER_CHUNK {
        for y in min_y / VOXELS_PER_CHUNK..=max_y / VOXELS_PER_CHUNK {
            for x in min_x / VOXELS_PER_CHUNK..=max_x / VOXELS_PER_CHUNK {
                let index = x + CHUNKS_PER_AXIS * y + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * z;
                mask[index / 32] |= 1 << (index % 32);
            }
        }
    }

    mask
}

#[derive(Clone, Copy, Debug)]
pub struct VoxelChunk {
    occupancy: [u32; WORDS_PER_CHUNK],
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self {
            occupancy: [0; WORDS_PER_CHUNK],
        }
    }
}

impl VoxelChunk {
    pub fn occupancy_words(&self) -> &[u32; WORDS_PER_CHUNK] {
        &self.occupancy
    }

    fn mark_occupied(&mut self, x: usize, y: usize, z: usize) {
        let block =
            (x >> 2) + BLOCKS_PER_AXIS * (y >> 2) + BLOCKS_PER_AXIS * BLOCKS_PER_AXIS * (z >> 2);
        let bit = (x & 3) + 4 * (y & 3) + 16 * (z & 3);
        let word = block * 2 + bit / u32::BITS as usize;
        let shift = bit % u32::BITS as usize;

        self.occupancy[word] |= 1 << shift;
    }
}

#[derive(Clone, Debug)]
pub struct VoxelRegion {
    chunks: [VoxelChunk; CHUNK_COUNT],
}

impl Default for VoxelRegion {
    fn default() -> Self {
        Self {
            chunks: [VoxelChunk::default(); CHUNK_COUNT],
        }
    }
}

impl VoxelRegion {
    pub fn voxelize(renderables: &[MeshInstance], meshes: &[MeshData]) -> Result<Self> {
        let mut region = Self::default();

        for instance in renderables {
            let mesh = meshes
                .get(instance.mesh.index())
                .with_context(|| format!("unknown mesh handle {}", instance.mesh.index()))?;
            ensure!(
                mesh.indices.len() % 3 == 0,
                "mesh index count is not a multiple of three"
            );
            ensure!(instance.model.is_finite(), "mesh model matrix is invalid");

            for indices in mesh.indices.chunks_exact(3) {
                let mut triangle = [Vec3::ZERO; 3];

                for (slot, index) in indices.iter().enumerate() {
                    let vertex = mesh
                        .vertices
                        .get(*index as usize)
                        .context("mesh index is out of bounds")?;
                    let position = instance
                        .model
                        .transform_point3(Vec3::from_array(vertex.position));
                    ensure!(position.is_finite(), "transformed mesh vertex is invalid");
                    triangle[slot] = position;
                }

                region.voxelize_triangle(triangle);
            }
        }

        Ok(region)
    }

    pub fn origin(&self) -> Vec3 {
        Vec3::from_array(REGION_ORIGIN)
    }

    pub fn occupied_voxel_count(&self) -> usize {
        let mut count = 0;

        for z in 0..CHUNKS_PER_AXIS {
            for y in 0..CHUNKS_PER_AXIS {
                for x in 0..CHUNKS_PER_AXIS {
                    count += self
                        .chunk([x, y, z])
                        .expect("chunk coordinates are in range")
                        .occupancy_words()
                        .iter()
                        .map(|word| word.count_ones() as usize)
                        .sum::<usize>();
                }
            }
        }

        count
    }

    pub fn occupied_voxel_centers(&self) -> Vec<Vec3> {
        let mut centers = Vec::with_capacity(self.occupied_voxel_count());
        let origin = self.origin();

        for z in 0..VOXELS_PER_AXIS {
            for y in 0..VOXELS_PER_AXIS {
                for x in 0..VOXELS_PER_AXIS {
                    if self.voxel_is_occupied(x, y, z) {
                        let coordinates = Vec3::new(x as f32, y as f32, z as f32);
                        centers.push(origin + (coordinates + Vec3::splat(0.5)) * VOXEL_SIZE_METERS);
                    }
                }
            }
        }

        centers
    }

    pub fn chunk(&self, coordinates: [usize; 3]) -> Option<&VoxelChunk> {
        let [x, y, z] = coordinates;
        if x >= CHUNKS_PER_AXIS || y >= CHUNKS_PER_AXIS || z >= CHUNKS_PER_AXIS {
            return None;
        }

        let index = x + CHUNKS_PER_AXIS * y + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * z;
        self.chunks.get(index)
    }

    fn voxel_is_occupied(&self, x: usize, y: usize, z: usize) -> bool {
        let chunk = &self.chunks[x / VOXELS_PER_CHUNK
            + CHUNKS_PER_AXIS * (y / VOXELS_PER_CHUNK)
            + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * (z / VOXELS_PER_CHUNK)];
        let x = x % VOXELS_PER_CHUNK;
        let y = y % VOXELS_PER_CHUNK;
        let z = z % VOXELS_PER_CHUNK;
        let block =
            (x >> 2) + BLOCKS_PER_AXIS * (y >> 2) + BLOCKS_PER_AXIS * BLOCKS_PER_AXIS * (z >> 2);
        let bit = (x & 3) + 4 * (y & 3) + 16 * (z & 3);
        let word = block * 2 + bit / u32::BITS as usize;
        let shift = bit % u32::BITS as usize;

        chunk.occupancy[word] & (1 << shift) != 0
    }

    fn voxelize_triangle(&mut self, triangle: [Vec3; 3]) {
        let triangle_min = triangle[0].min(triangle[1]).min(triangle[2]);
        let triangle_max = triangle[0].max(triangle[1]).max(triangle[2]);
        let origin = self.origin();

        let Some((min_x, max_x)) = candidate_range(triangle_min.x, triangle_max.x, origin.x) else {
            return;
        };
        let Some((min_y, max_y)) = candidate_range(triangle_min.y, triangle_max.y, origin.y) else {
            return;
        };
        let Some((min_z, max_z)) = candidate_range(triangle_min.z, triangle_max.z, origin.z) else {
            return;
        };

        for z in min_z..=max_z {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let voxel_min =
                        origin + Vec3::new(x as f32, y as f32, z as f32) * VOXEL_SIZE_METERS;
                    let voxel_max = voxel_min + Vec3::splat(VOXEL_SIZE_METERS);

                    if triangle_intersects_box(triangle, voxel_min, voxel_max) {
                        self.mark_occupied(x, y, z);
                    }
                }
            }
        }
    }

    fn mark_occupied(&mut self, x: usize, y: usize, z: usize) {
        let chunk_index = x / VOXELS_PER_CHUNK
            + CHUNKS_PER_AXIS * (y / VOXELS_PER_CHUNK)
            + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * (z / VOXELS_PER_CHUNK);
        let local_x = x % VOXELS_PER_CHUNK;
        let local_y = y % VOXELS_PER_CHUNK;
        let local_z = z % VOXELS_PER_CHUNK;

        self.chunks[chunk_index].mark_occupied(local_x, local_y, local_z);
    }
}

fn candidate_range(min: f32, max: f32, origin: f32) -> Option<(usize, usize)> {
    candidate_range_with_size(min, max, origin, VOXEL_SIZE_METERS)
}

fn candidate_range_with_size(
    min: f32,
    max: f32,
    origin: f32,
    voxel_size: f32,
) -> Option<(usize, usize)> {
    let first = (((min - origin) / voxel_size).floor() as i32)
        .saturating_sub(1)
        .max(0);
    let last = (((max - origin) / voxel_size).floor() as i32).min(VOXELS_PER_AXIS as i32 - 1);

    (first <= last).then_some((first as usize, last as usize))
}

fn triangle_intersects_box(triangle: [Vec3; 3], min: Vec3, max: Vec3) -> bool {
    let center = (min + max) * 0.5;
    let half_size = (max - min) * 0.5;
    let vertices = triangle.map(|vertex| vertex - center);
    let edges = [
        vertices[1] - vertices[0],
        vertices[2] - vertices[1],
        vertices[0] - vertices[2],
    ];
    let box_axes = [Vec3::X, Vec3::Y, Vec3::Z];

    for axis in box_axes {
        if separates_on_axis(axis, vertices, half_size) {
            return false;
        }
    }

    for edge in edges {
        for axis in box_axes {
            if separates_on_axis(edge.cross(axis), vertices, half_size) {
                return false;
            }
        }
    }

    !separates_on_axis(edges[0].cross(edges[1]), vertices, half_size)
}

fn separates_on_axis(axis: Vec3, triangle: [Vec3; 3], half_size: Vec3) -> bool {
    if axis.length_squared() <= 1.0e-12 {
        return false;
    }

    let projection_0 = triangle[0].dot(axis);
    let projection_1 = triangle[1].dot(axis);
    let projection_2 = triangle[2].dot(axis);
    let triangle_min = projection_0.min(projection_1).min(projection_2);
    let triangle_max = projection_0.max(projection_1).max(projection_2);
    let box_radius = half_size.dot(axis.abs());

    triangle_min > box_radius || triangle_max < -box_radius
}
