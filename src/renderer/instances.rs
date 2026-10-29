use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4};

use crate::{handle::MeshHandle, scene::MeshInstance};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct InstanceRaw {
    model: [[f32; 4]; 4],
    normal: [[f32; 4]; 3],
    albedo: [f32; 4],
    emission: [f32; 4],
}

const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4,
    6 => Float32x4,
    7 => Float32x4,
    8 => Float32x4,
    9 => Float32x4,
    10 => Float32x4
];

impl InstanceRaw {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    };

    fn from_model(model: Mat4, albedo: glam::Vec3, emission: glam::Vec3) -> Result<Self> {
        let linear = Mat3::from_mat4(model);
        let determinant = linear.determinant();

        ensure!(
            determinant.is_finite() && determinant.abs() > f32::EPSILON,
            "mesh transform has a singular normal matrix"
        );

        let normal = linear.inverse().transpose().to_cols_array_2d();

        Ok(Self {
            model: model.to_cols_array_2d(),
            normal: [
                [normal[0][0], normal[0][1], normal[0][2], 0.0],
                [normal[1][0], normal[1][1], normal[1][2], 0.0],
                [normal[2][0], normal[2][1], normal[2][2], 0.0],
            ],
            albedo: [albedo.x, albedo.y, albedo.z, 1.0],
            emission: [emission.x, emission.y, emission.z, 1.0],
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MeshBatch {
    pub mesh: MeshHandle,
    pub first_instance: u32,
    pub instance_count: u32,
}

pub struct InstanceBuffer {
    buffer: wgpu::Buffer,
    capacity: usize,
}

impl InstanceBuffer {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let capacity = 1;

        Ok(Self {
            buffer: Self::create_buffer(device, capacity)?,
            capacity,
        })
    }

    pub fn write(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderables: &[MeshInstance],
    ) -> Result<Vec<MeshBatch>> {
        u32::try_from(renderables.len()).context("too many mesh instances")?;

        let mut groups = BTreeMap::<MeshHandle, Vec<(Mat4, glam::Vec3, glam::Vec3)>>::new();

        for instance in renderables {
            groups.entry(instance.mesh).or_default().push((
                instance.model,
                instance.albedo,
                instance.emission,
            ));
        }

        let mut instances = Vec::with_capacity(renderables.len());
        let mut batches = Vec::with_capacity(groups.len());

        for (mesh, models) in groups {
            let first_instance =
                u32::try_from(instances.len()).context("too many mesh instances")?;
            let instance_count = u32::try_from(models.len()).context("too many mesh instances")?;

            for (model, albedo, emission) in models {
                instances.push(InstanceRaw::from_model(model, albedo, emission)?);
            }

            batches.push(MeshBatch {
                mesh,
                first_instance,
                instance_count,
            });
        }

        self.ensure_capacity(device, instances.len())?;

        if !instances.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&instances));
        }

        Ok(batches)
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    fn ensure_capacity(&mut self, device: &wgpu::Device, required: usize) -> Result<()> {
        if required <= self.capacity {
            return Ok(());
        }

        let capacity = required
            .checked_next_power_of_two()
            .context("instance buffer capacity overflow")?;
        let buffer = Self::create_buffer(device, capacity)?;
        self.buffer = buffer;
        self.capacity = capacity;

        Ok(())
    }

    fn create_buffer(device: &wgpu::Device, capacity: usize) -> Result<wgpu::Buffer> {
        let size = capacity
            .checked_mul(std::mem::size_of::<InstanceRaw>())
            .and_then(|size| u64::try_from(size).ok())
            .context("instance buffer size overflow")?;

        Ok(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mesh instance buffer"),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }))
    }
}
