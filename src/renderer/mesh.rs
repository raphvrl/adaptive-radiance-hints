use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

pub mod gpu;

pub use gpu::GpuMesh;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl MeshVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x3
        ],
    };
}

#[derive(Debug)]
pub struct MeshData {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl Aabb {
    pub fn from_vertices(vertices: &[MeshVertex]) -> Result<Self> {
        let first = vertices
            .first()
            .context("cannot compute bounds for an empty mesh")?;
        let first = Vec3::from_array(first.position);
        ensure!(
            first.is_finite(),
            "mesh contains an invalid vertex position"
        );

        let mut bounds = Self {
            min: first,
            max: first,
        };

        for vertex in &vertices[1..] {
            let position = Vec3::from_array(vertex.position);
            ensure!(
                position.is_finite(),
                "mesh contains an invalid vertex position"
            );

            bounds.min = bounds.min.min(position);
            bounds.max = bounds.max.max(position);
        }

        Ok(bounds)
    }

    pub fn transformed(self, model: Mat4) -> Result<Self> {
        ensure!(model.is_finite(), "mesh model matrix is invalid");

        let first_corner = model.transform_point3(self.min);
        ensure!(
            first_corner.is_finite(),
            "transformed mesh bounds are invalid"
        );

        let mut bounds = Self {
            min: first_corner,
            max: first_corner,
        };

        for index in 1..8 {
            let corner = Vec3::new(
                if index & 1 == 0 {
                    self.min.x
                } else {
                    self.max.x
                },
                if index & 2 == 0 {
                    self.min.y
                } else {
                    self.max.y
                },
                if index & 4 == 0 {
                    self.min.z
                } else {
                    self.max.z
                },
            );
            let corner = model.transform_point3(corner);
            ensure!(corner.is_finite(), "transformed mesh bounds are invalid");

            bounds.min = bounds.min.min(corner);
            bounds.max = bounds.max.max(corner);
        }

        Ok(bounds)
    }
}
