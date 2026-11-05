use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use super::{Aabb, MeshData};

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VoxelTriangle {
    a: [f32; 4],
    b: [f32; 4],
    c: [f32; 4],
}

pub struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    voxel_triangle_buffer: wgpu::Buffer,
    index_count: u32,
    voxel_triangle_count: u32,
    bounds: Aabb,
}

impl GpuMesh {
    pub fn upload(device: &wgpu::Device, data: &MeshData) -> Result<Self> {
        ensure!(
            !data.vertices.is_empty() && !data.indices.is_empty(),
            "cannot upload an empty mesh"
        );

        let index_count =
            u32::try_from(data.indices.len()).context("mesh has more than u32::MAX indices")?;
        let bounds = Aabb::from_vertices(&data.vertices)?;
        ensure!(
            data.indices.len() % 3 == 0,
            "mesh index count is not a multiple of three"
        );

        let mut voxel_triangles = Vec::with_capacity(data.indices.len() / 3);
        for indices in data.indices.chunks_exact(3) {
            let mut positions = [[0.0; 4]; 3];

            for (slot, index) in indices.iter().enumerate() {
                let vertex = data
                    .vertices
                    .get(*index as usize)
                    .context("mesh index is out of bounds")?;
                positions[slot] = Vec3::from_array(vertex.position).extend(1.0).to_array();
            }

            voxel_triangles.push(VoxelTriangle {
                a: positions[0],
                b: positions[1],
                c: positions[2],
            });
        }
        let voxel_triangle_count =
            u32::try_from(voxel_triangles.len()).context("too many mesh triangles")?;

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh vertex buffer"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh index buffer"),
            contents: bytemuck::cast_slice(&data.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let voxel_triangle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Mesh voxel triangles"),
            contents: bytemuck::cast_slice(&voxel_triangles),
            usage: wgpu::BufferUsages::STORAGE,
        });

        Ok(Self {
            vertex_buffer,
            index_buffer,
            voxel_triangle_buffer,
            index_count,
            voxel_triangle_count,
            bounds,
        })
    }

    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.index_buffer
    }

    pub fn voxel_triangle_buffer(&self) -> &wgpu::Buffer {
        &self.voxel_triangle_buffer
    }

    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    pub fn voxel_triangle_count(&self) -> u32 {
        self.voxel_triangle_count
    }

    pub fn bounds(&self) -> Aabb {
        self.bounds
    }
}
