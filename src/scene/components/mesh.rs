use crate::handle::MeshHandle;

#[derive(Clone, Debug)]
pub struct Mesh {
    pub handle: MeshHandle,
    pub asset_id: String,
}
