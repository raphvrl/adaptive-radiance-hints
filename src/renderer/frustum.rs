use anyhow::{Result, ensure};
use glam::{Mat4, Vec3, Vec4};

use crate::renderer::mesh::Aabb;

const INTERSECTION_EPSILON: f32 = 0.001;

#[derive(Clone, Copy)]
pub struct Frustum {
    planes: [Vec4; 6],
    corners: [Vec3; 8],
}

impl Frustum {
    pub fn from_view_proj(view_proj: Mat4) -> Result<Self> {
        ensure!(
            view_proj.is_finite(),
            "frustum view-projection matrix is invalid"
        );

        let inverse = view_proj.inverse();
        ensure!(
            inverse.is_finite(),
            "frustum view-projection matrix is singular"
        );

        let corners = [
            Self::unproject_corner(inverse, Vec4::new(-1.0, -1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, -1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, 1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, 1.0, 0.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, -1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, -1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(-1.0, 1.0, 1.0, 1.0))?,
            Self::unproject_corner(inverse, Vec4::new(1.0, 1.0, 1.0, 1.0))?,
        ];

        let columns = view_proj.to_cols_array_2d();
        let row = |index| {
            Vec4::new(
                columns[0][index],
                columns[1][index],
                columns[2][index],
                columns[3][index],
            )
        };
        let row_0 = row(0);
        let row_1 = row(1);
        let row_2 = row(2);
        let row_3 = row(3);
        let planes = [
            Self::normalize_plane(row_3 + row_0)?,
            Self::normalize_plane(row_3 - row_0)?,
            Self::normalize_plane(row_3 + row_1)?,
            Self::normalize_plane(row_3 - row_1)?,
            Self::normalize_plane(row_2)?,
            Self::normalize_plane(row_3 - row_2)?,
        ];

        Ok(Self { planes, corners })
    }

    pub fn intersects(self, other: Self) -> bool {
        !self.separates(other) && !other.separates(self)
    }

    pub fn intersects_aabb(self, bounds: Aabb) -> bool {
        self.planes.iter().all(|plane| {
            let positive_vertex = Vec3::new(
                if plane.x >= 0.0 {
                    bounds.max.x
                } else {
                    bounds.min.x
                },
                if plane.y >= 0.0 {
                    bounds.max.y
                } else {
                    bounds.min.y
                },
                if plane.z >= 0.0 {
                    bounds.max.z
                } else {
                    bounds.min.z
                },
            );

            plane.dot(positive_vertex.extend(1.0)) >= -INTERSECTION_EPSILON
        })
    }

    fn separates(self, other: Self) -> bool {
        self.planes.iter().any(|plane| {
            other
                .corners
                .iter()
                .all(|corner| plane.dot(corner.extend(1.0)) < -INTERSECTION_EPSILON)
        })
    }

    fn unproject_corner(inverse: Mat4, clip_corner: Vec4) -> Result<Vec3> {
        let world_corner = inverse * clip_corner;
        ensure!(
            world_corner.is_finite() && world_corner.w.abs() > f32::EPSILON,
            "frustum corner is invalid"
        );

        let world_corner = world_corner.truncate() / world_corner.w;
        ensure!(world_corner.is_finite(), "frustum corner is invalid");

        Ok(world_corner)
    }

    fn normalize_plane(plane: Vec4) -> Result<Vec4> {
        let normal_length = plane.truncate().length();
        ensure!(
            normal_length.is_finite() && normal_length > f32::EPSILON,
            "frustum plane is invalid"
        );

        Ok(plane / normal_length)
    }
}
