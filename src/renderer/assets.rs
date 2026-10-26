use anyhow::{Context, Result};

use crate::{
    handle::MeshHandle,
    renderer::mesh::{GpuMesh, MeshData},
};

#[derive(Default)]
pub struct MeshAssets {
    meshes: Vec<GpuMesh>,
}

impl MeshAssets {
    pub fn insert(&mut self, device: &wgpu::Device, data: &MeshData) -> Result<MeshHandle> {
        let index = u32::try_from(self.meshes.len()).context("too many meshes")?;
        let mesh = GpuMesh::upload(device, data)?;

        self.meshes.push(mesh);

        Ok(MeshHandle::from_index(index))
    }

    pub fn get(&self, handle: MeshHandle) -> Result<&GpuMesh> {
        self.meshes
            .get(handle.index())
            .with_context(|| format!("unknown mesh handle {}", handle.index()))
    }
}
