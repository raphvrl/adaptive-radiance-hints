use std::collections::HashMap;

use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct MaterialId(pub String);

impl MaterialId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub albedo: Vec3,
    pub emission: Vec3,
}

#[derive(Default)]
pub struct MaterialLibrary {
    materials: HashMap<MaterialId, Material>,
}

impl MaterialLibrary {
    pub fn get(&self, id: &MaterialId) -> Option<&Material> {
        self.materials.get(id)
    }

    pub fn get_mut(&mut self, id: &MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id)
    }

    pub fn insert(&mut self, id: MaterialId, material: Material) -> Option<Material> {
        self.materials.insert(id, material)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&MaterialId, &Material)> {
        self.materials.iter()
    }
}
