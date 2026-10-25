use crate::renderer::mesh::{MeshData, MeshVertex};

const CORNERS: [[f32; 3]; 8] = [
    [-0.5, -0.5, -0.5], // 0
    [0.5, -0.5, -0.5],  // 1
    [0.5, 0.5, -0.5],   // 2
    [-0.5, 0.5, -0.5],  // 3
    [-0.5, -0.5, 0.5],  // 4
    [0.5, -0.5, 0.5],   // 5
    [0.5, 0.5, 0.5],    // 6
    [-0.5, 0.5, 0.5],   // 7
];

struct Face {
    normal: [f32; 3],
    corners: [usize; 4],
}

const FACES: [Face; 6] = [
    Face {
        normal: [0.0, 0.0, 1.0],
        corners: [4, 5, 6, 7],
    },
    Face {
        normal: [0.0, 0.0, -1.0],
        corners: [1, 0, 3, 2],
    },
    Face {
        normal: [1.0, 0.0, 0.0],
        corners: [5, 1, 2, 6],
    },
    Face {
        normal: [-1.0, 0.0, 0.0],
        corners: [0, 4, 7, 3],
    },
    Face {
        normal: [0.0, 1.0, 0.0],
        corners: [7, 6, 2, 3],
    },
    Face {
        normal: [0.0, -1.0, 0.0],
        corners: [0, 1, 5, 4],
    },
];

pub fn cube() -> MeshData {
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(24),
        indices: Vec::with_capacity(36),
    };

    for face in FACES {
        let base = mesh.vertices.len() as u32;

        mesh.vertices.extend(face.corners.map(|corner| MeshVertex {
            position: CORNERS[corner],
            normal: face.normal,
        }));

        #[rustfmt::skip]
        mesh.indices.extend([
            base,
            base + 1,
            base + 2,
            base,
            base + 2,
            base + 3,
        ]);
    }

    mesh
}
