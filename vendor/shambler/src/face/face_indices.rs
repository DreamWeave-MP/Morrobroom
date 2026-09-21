use std::{cmp::Ordering, collections::BTreeMap};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::{FaceCenters, FaceId, FaceVertices};
use crate::{vector3_from_point, FacePlanes, FaceTrianglePlanes};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FaceWinding {
    Clockwise,
    CounterClockwise,
}

pub enum FaceIndicesTag {}

pub type FaceIndices = Usage<FaceIndicesTag, BTreeMap<FaceId, Vec<usize>>>;

// Generate face indices with the specified winding
pub fn face_indices(
    face_planes: &FaceTrianglePlanes,
    geo_planes: &FacePlanes,
    face_vertices: &FaceVertices,
    face_centers: &FaceCenters,
    winding: FaceWinding,
) -> FaceIndices {
    face_vertices
        .par_iter()
        .map(|(plane_id, vertices)| {
            let face_plane = &face_planes[plane_id];
            let plane = &geo_planes[plane_id];
            let plane_center = &face_centers[plane_id];

            let plane_v0 = vector3_from_point(face_plane.v0);
            let plane_v1 = vector3_from_point(face_plane.v1);
            let u_axis = (plane_v1 - plane_v0).normalize();
            let v_axis = plane.normal().cross(&u_axis);

            let mut indices = (0..vertices.len()).collect::<Vec<_>>();
            indices.sort_unstable_by(|lhs, rhs| {
                let lhs_v = vertices[*lhs] - plane_center;
                let rhs_v = vertices[*rhs] - plane_center;

                let lhs_pu = lhs_v.dot(&u_axis);
                let lhs_pv = lhs_v.dot(&v_axis);

                let rhs_pu = rhs_v.dot(&u_axis);
                let rhs_pv = rhs_v.dot(&v_axis);

                let lhs_angle = lhs_pv.atan2(lhs_pu);
                let rhs_angle = rhs_pv.atan2(rhs_pu);

                if winding == FaceWinding::CounterClockwise {
                    rhs_angle.partial_cmp(&lhs_angle)
                } else {
                    lhs_angle.partial_cmp(&rhs_angle)
                }
                .unwrap_or(Ordering::Equal)
            });
            (*plane_id, indices)
        })
        .collect()
}
