use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4, Vec3};
use hecs::Entity;
use wesl_core::include_wesl;
use wgpu::util::DeviceExt;

use crate::{
    handle::MeshHandle,
    renderer::{
        assets::MeshAssets,
        mesh::{Aabb, GpuMesh},
        voxel::{self, CHUNK_COUNT, CHUNKS_PER_AXIS, DIRTY_WORDS, WORDS_PER_CHUNK},
    },
    scene::MeshInstance,
};

const WORKGROUP_SIZE: u32 = 64;

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VoxelParams {
    dirty_chunks: [u32; DIRTY_WORDS],
    ring_offset: [u32; 4],
    region_origin: [f32; 4],
}

#[derive(Clone, Copy)]
struct PreviousInstance {
    mesh: MeshHandle,
    model: Mat4,
    bounds: Aabb,
}

struct VoxelMeshResources {
    triangle_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_bind_group: wgpu::BindGroup,
    instance_capacity: usize,
    triangle_count: u32,
}

struct VoxelCascade {
    voxel_size: f32,
    clear_pipeline: wgpu::ComputePipeline,
    voxelize_pipeline: wgpu::ComputePipeline,
    params_buffer: wgpu::Buffer,
    dirty_list_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    triangle_layout: wgpu::BindGroupLayout,
    instance_layout: wgpu::BindGroupLayout,
    occupancy_buffer: wgpu::Buffer,
    occupancy_bind_group: wgpu::BindGroup,
    debug_buffer: wgpu::Buffer,
    mesh_resources: HashMap<MeshHandle, VoxelMeshResources>,
    dirty_chunks: [u32; DIRTY_WORDS],
    base_chunk: Option<IVec3>,
    ring_offset: [u32; 3],
    region_origin: Vec3,
    previous_instances: HashMap<Entity, PreviousInstance>,
}

impl VoxelCascade {
    fn new(device: &wgpu::Device, voxel_size: f32) -> Self {
        let params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel compute params layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_layout_entry(1, true),
            ],
        });
        let triangle_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel triangle storage layout"),
            entries: &[storage_layout_entry(0, true)],
        });
        let instance_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel instance storage layout"),
            entries: &[storage_layout_entry(0, true)],
        });
        let occupancy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Voxel occupancy storage layout"),
            entries: &[storage_layout_entry(0, false)],
        });
        let clear_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Clear voxel chunks pipeline layout"),
                bind_group_layouts: &[Some(&params_layout), Some(&occupancy_layout)],
                immediate_size: 0,
            });
        let voxelize_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Voxelization pipeline layout"),
                bind_group_layouts: &[
                    Some(&params_layout),
                    Some(&occupancy_layout),
                    Some(&triangle_layout),
                    Some(&instance_layout),
                ],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Voxelization compute shader"),
            source: wgpu::ShaderSource::Wgsl(include_wesl!("voxelize").into()),
        });
        let clear_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Clear dirty voxel chunks"),
            layout: Some(&clear_pipeline_layout),
            module: &shader,
            entry_point: Some("clear_dirty_chunks"),
            compilation_options: Default::default(),
            cache: None,
        });
        let voxelize_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Voxelize dirty chunks"),
            layout: Some(&voxelize_pipeline_layout),
            module: &shader,
            entry_point: Some("voxelize_triangles"),
            compilation_options: Default::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Voxel compute params"),
            contents: bytemuck::bytes_of(&VoxelParams::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let dirty_list_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Voxel dirty chunk indices"),
            size: (CHUNK_COUNT * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Voxel compute params bind group"),
            layout: &params_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dirty_list_buffer.as_entire_binding(),
                },
            ],
        });

        let occupancy_size = (CHUNK_COUNT * WORDS_PER_CHUNK * std::mem::size_of::<u32>()) as u64;
        let occupancy_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Voxel occupancy atomic words"),
            size: occupancy_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let occupancy_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Voxel occupancy bind group"),
            layout: &occupancy_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: occupancy_buffer.as_entire_binding(),
            }],
        });
        let debug_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Voxel debug occupancy words"),
            size: occupancy_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            voxel_size,
            clear_pipeline,
            voxelize_pipeline,
            params_buffer,
            dirty_list_buffer,
            params_bind_group,
            triangle_layout,
            instance_layout,
            occupancy_buffer,
            occupancy_bind_group,
            debug_buffer,
            mesh_resources: HashMap::new(),
            dirty_chunks: [0; DIRTY_WORDS],
            base_chunk: None,
            ring_offset: [0; 3],
            region_origin: Vec3::ZERO,
            previous_instances: HashMap::new(),
        }
    }

    pub fn debug_buffer(&self) -> &wgpu::Buffer {
        &self.debug_buffer
    }

    pub fn params_buffer(&self) -> &wgpu::Buffer {
        &self.params_buffer
    }

    pub fn register_mesh(&mut self, device: &wgpu::Device, handle: MeshHandle, mesh: &GpuMesh) {
        let triangle_bind_group = create_mesh_bind_group(
            device,
            &self.triangle_layout,
            mesh.voxel_triangle_buffer(),
            "Voxel mesh triangle bind group",
        );
        let instance_capacity = 1;
        let instance_buffer = create_instance_buffer(device, instance_capacity);
        let instance_bind_group = create_mesh_bind_group(
            device,
            &self.instance_layout,
            &instance_buffer,
            "Voxel mesh instance bind group",
        );

        self.mesh_resources.insert(
            handle,
            VoxelMeshResources {
                triangle_bind_group,
                instance_buffer,
                instance_bind_group,
                instance_capacity,
                triangle_count: mesh.voxel_triangle_count(),
            },
        );
    }

    pub fn update_view_region(&mut self, camera_position: Vec3) -> Result<()> {
        ensure!(camera_position.is_finite(), "camera position is invalid");

        let chunk_offset = Vec3::from_array(voxel::REGION_CHUNK_OFFSET);
        let camera_chunk = ((camera_position - chunk_offset)
            / (self.voxel_size * voxel::VOXELS_PER_CHUNK as f32))
            .floor()
            .as_ivec3();
        let base_chunk = camera_chunk - IVec3::splat((CHUNKS_PER_AXIS / 2) as i32);
        if self.base_chunk == Some(base_chunk) {
            return Ok(());
        }

        let mut dirty_chunks = [0; DIRTY_WORDS];

        for z in 0..CHUNKS_PER_AXIS {
            for y in 0..CHUNKS_PER_AXIS {
                for x in 0..CHUNKS_PER_AXIS {
                    let world_chunk = base_chunk + IVec3::new(x as i32, y as i32, z as i32);
                    let remains_in_window = self.base_chunk.is_some_and(|previous_base| {
                        world_chunk.x >= previous_base.x
                            && world_chunk.x < previous_base.x + CHUNKS_PER_AXIS as i32
                            && world_chunk.y >= previous_base.y
                            && world_chunk.y < previous_base.y + CHUNKS_PER_AXIS as i32
                            && world_chunk.z >= previous_base.z
                            && world_chunk.z < previous_base.z + CHUNKS_PER_AXIS as i32
                    });

                    if !remains_in_window {
                        let logical_index =
                            x + CHUNKS_PER_AXIS * y + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * z;
                        dirty_chunks[logical_index / 32] |= 1 << (logical_index % 32);
                    }
                }
            }
        }

        self.base_chunk = Some(base_chunk);
        self.ring_offset = [
            base_chunk.x.rem_euclid(CHUNKS_PER_AXIS as i32) as u32,
            base_chunk.y.rem_euclid(CHUNKS_PER_AXIS as i32) as u32,
            base_chunk.z.rem_euclid(CHUNKS_PER_AXIS as i32) as u32,
        ];
        self.region_origin = Vec3::new(
            base_chunk.x as f32,
            base_chunk.y as f32,
            base_chunk.z as f32,
        ) * (self.voxel_size * voxel::VOXELS_PER_CHUNK as f32)
            + chunk_offset;
        merge_dirty(&mut self.dirty_chunks, dirty_chunks);

        Ok(())
    }

    pub fn mark_scene_changed(
        &mut self,
        renderables: &[MeshInstance],
        assets: &MeshAssets,
    ) -> Result<()> {
        let mut current_instances = HashMap::with_capacity(renderables.len());
        let mut dirty_chunks = [0; DIRTY_WORDS];

        for instance in renderables {
            let bounds = assets
                .get(instance.mesh)?
                .bounds()
                .transformed(instance.model)?;
            let current = PreviousInstance {
                mesh: instance.mesh,
                model: instance.model,
                bounds,
            };

            match self.previous_instances.get(&instance.entity) {
                Some(previous)
                    if previous.mesh != current.mesh || previous.model != current.model =>
                {
                    merge_dirty(
                        &mut dirty_chunks,
                        voxel::chunk_mask_for_bounds(
                            previous.bounds,
                            self.region_origin,
                            self.voxel_size,
                        ),
                    );
                    merge_dirty(
                        &mut dirty_chunks,
                        voxel::chunk_mask_for_bounds(
                            current.bounds,
                            self.region_origin,
                            self.voxel_size,
                        ),
                    );
                }
                None => {
                    merge_dirty(
                        &mut dirty_chunks,
                        voxel::chunk_mask_for_bounds(
                            current.bounds,
                            self.region_origin,
                            self.voxel_size,
                        ),
                    );
                }
                _ => {}
            }

            current_instances.insert(instance.entity, current);
        }

        for (entity, previous) in &self.previous_instances {
            if !current_instances.contains_key(entity) {
                merge_dirty(
                    &mut dirty_chunks,
                    voxel::chunk_mask_for_bounds(
                        previous.bounds,
                        self.region_origin,
                        self.voxel_size,
                    ),
                );
            }
        }

        self.previous_instances = current_instances;
        merge_dirty(&mut self.dirty_chunks, dirty_chunks);

        Ok(())
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderables: &[MeshInstance],
    ) -> Result<()> {
        if self.dirty_chunks.iter().all(|word| *word == 0) {
            return Ok(());
        }

        let dirty_indices: Vec<u32> = (0..CHUNK_COUNT)
            .filter(|index| self.dirty_chunks[index / 32] & (1 << (index % 32)) != 0)
            .map(|index| index as u32)
            .collect();
        queue.write_buffer(
            &self.dirty_list_buffer,
            0,
            bytemuck::cast_slice(&dirty_indices),
        );
        let dirty_count = dirty_indices.len() as u32;

        let mut models_by_mesh = BTreeMap::<MeshHandle, Vec<[[f32; 4]; 4]>>::new();
        for instance in renderables {
            ensure!(instance.model.is_finite(), "mesh model matrix is invalid");
            models_by_mesh
                .entry(instance.mesh)
                .or_default()
                .push(instance.model.to_cols_array_2d());
        }

        let instance_layout = &self.instance_layout;
        for (mesh_handle, models) in &models_by_mesh {
            let resources = self
                .mesh_resources
                .get_mut(mesh_handle)
                .with_context(|| format!("unknown voxel mesh handle {}", mesh_handle.index()))?;
            if models.len() > resources.instance_capacity {
                let matrix_size = std::mem::size_of::<[[f32; 4]; 4]>();
                let max_capacity = (u64::from(device.limits().max_storage_buffer_binding_size)
                    / matrix_size as u64) as usize;
                ensure!(
                    models.len() <= max_capacity,
                    "voxel instance buffer exceeds the device storage binding limit"
                );

                resources.instance_capacity = models
                    .len()
                    .checked_next_power_of_two()
                    .unwrap_or(models.len())
                    .min(max_capacity)
                    .max(models.len());
                resources.instance_buffer =
                    create_instance_buffer(device, resources.instance_capacity);
                resources.instance_bind_group = create_mesh_bind_group(
                    device,
                    instance_layout,
                    &resources.instance_buffer,
                    "Voxel mesh instance bind group",
                );
            }

            queue.write_buffer(&resources.instance_buffer, 0, bytemuck::cast_slice(models));

            let workgroups_x = resources.triangle_count.div_ceil(WORKGROUP_SIZE);
            let instance_count =
                u32::try_from(models.len()).context("too many voxel mesh instances")?;
            let workgroup_limit = device.limits().max_compute_workgroups_per_dimension;
            ensure!(
                workgroups_x <= workgroup_limit && instance_count <= workgroup_limit,
                "voxelization dispatch exceeds the device workgroup limit"
            );
        }

        queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::bytes_of(&VoxelParams {
                dirty_chunks: self.dirty_chunks,
                ring_offset: [
                    self.ring_offset[0],
                    self.ring_offset[1],
                    self.ring_offset[2],
                    0,
                ],
                region_origin: [
                    self.region_origin.x,
                    self.region_origin.y,
                    self.region_origin.z,
                    self.voxel_size,
                ],
            }),
        );

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Clear dirty voxel chunks"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.clear_pipeline);
            pass.set_bind_group(0, &self.params_bind_group, &[]);
            pass.set_bind_group(1, &self.occupancy_bind_group, &[]);
            pass.dispatch_workgroups((WORDS_PER_CHUNK as u32).div_ceil(16), dirty_count, 1);
        }

        if !models_by_mesh.is_empty() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Voxelize dirty chunks"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.voxelize_pipeline);
            pass.set_bind_group(0, &self.params_bind_group, &[]);
            pass.set_bind_group(1, &self.occupancy_bind_group, &[]);

            for (mesh_handle, models) in &models_by_mesh {
                let resources = self
                    .mesh_resources
                    .get(mesh_handle)
                    .context("voxel mesh resources disappeared")?;
                let workgroups_x = resources.triangle_count.div_ceil(WORKGROUP_SIZE);

                pass.set_bind_group(2, &resources.triangle_bind_group, &[]);
                pass.set_bind_group(3, &resources.instance_bind_group, &[]);
                pass.dispatch_workgroups(workgroups_x, dirty_count, models.len() as u32);
            }
        }

        let chunk_size = (WORDS_PER_CHUNK * std::mem::size_of::<u32>()) as u64;
        for chunk_index in 0..CHUNK_COUNT {
            if self.dirty_chunks[chunk_index / 32] & (1 << (chunk_index % 32)) == 0 {
                continue;
            }

            let physical_index = physical_chunk_index(chunk_index, self.ring_offset);
            let offset = physical_index as u64 * chunk_size;
            encoder.copy_buffer_to_buffer(
                &self.occupancy_buffer,
                offset,
                &self.debug_buffer,
                offset,
                chunk_size,
            );
        }

        self.dirty_chunks = [0; DIRTY_WORDS];

        Ok(())
    }
}

pub struct VoxelComputePass {
    cascades: [VoxelCascade; 2],
}

impl VoxelComputePass {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            cascades: [
                VoxelCascade::new(device, voxel::VOXEL_SIZE_METERS),
                VoxelCascade::new(device, voxel::VOXEL_SIZE_METERS * 2.0),
            ],
        }
    }

    pub fn debug_buffers(&self) -> impl Iterator<Item = (&wgpu::Buffer, &wgpu::Buffer)> {
        self.cascades
            .iter()
            .map(|cascade| (cascade.debug_buffer(), cascade.params_buffer()))
    }

    pub fn register_mesh(&mut self, device: &wgpu::Device, handle: MeshHandle, mesh: &GpuMesh) {
        for cascade in &mut self.cascades {
            cascade.register_mesh(device, handle, mesh);
        }
    }

    pub fn update_view_region(&mut self, camera_position: Vec3) -> Result<()> {
        for cascade in &mut self.cascades {
            cascade.update_view_region(camera_position)?;
        }
        Ok(())
    }

    pub fn mark_scene_changed(
        &mut self,
        renderables: &[MeshInstance],
        assets: &MeshAssets,
    ) -> Result<()> {
        for cascade in &mut self.cascades {
            cascade.mark_scene_changed(renderables, assets)?;
        }
        Ok(())
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        renderables: &[MeshInstance],
    ) -> Result<()> {
        for cascade in &mut self.cascades {
            cascade.encode(device, queue, encoder, renderables)?;
        }
        Ok(())
    }
}

fn merge_dirty(target: &mut [u32; DIRTY_WORDS], source: [u32; DIRTY_WORDS]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
}

fn physical_chunk_index(logical_index: usize, ring_offset: [u32; 3]) -> usize {
    let x = logical_index % CHUNKS_PER_AXIS;
    let y = logical_index / CHUNKS_PER_AXIS % CHUNKS_PER_AXIS;
    let z = logical_index / (CHUNKS_PER_AXIS * CHUNKS_PER_AXIS);
    let [offset_x, offset_y, offset_z] = ring_offset.map(|offset| offset as usize);

    (x + offset_x) % CHUNKS_PER_AXIS
        + CHUNKS_PER_AXIS * ((y + offset_y) % CHUNKS_PER_AXIS)
        + CHUNKS_PER_AXIS * CHUNKS_PER_AXIS * ((z + offset_z) % CHUNKS_PER_AXIS)
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Voxel mesh instance matrices"),
        size: (capacity * std::mem::size_of::<[[f32; 4]; 4]>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_mesh_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    label: &'static str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

fn storage_layout_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
