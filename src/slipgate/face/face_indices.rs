use std::cmp::Ordering;

use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use usage::Usage;

use super::{FaceCenters, FaceId, FaceVertices};
use crate::slipgate::{DenseStorage, FacePlanes, FaceTrianglePlanes, vector3_from_point};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FaceWinding {
    Clockwise,
    CounterClockwise,
}

pub enum FaceIndicesTag {}

pub type FaceIndices = Usage<FaceIndicesTag, DenseStorage<FaceId, Vec<usize>>>;

// Generate face indices with the specified winding
pub fn face_indices(
    face_planes: &FaceTrianglePlanes,
    geo_planes: &FacePlanes,
    face_vertices: &FaceVertices,
    face_centers: &FaceCenters,
    winding: FaceWinding,
) -> FaceIndices {
    let indices = face_vertices
        .par_iter()
        .enumerate()
        .map(|(face_index, vertices)| {
            let plane_id = FaceId(face_index);
            let face_plane = &face_planes[&plane_id];
            let plane = &geo_planes[plane_id];
            let plane_center = &face_centers[plane_id];

            let plane_v0 = vector3_from_point(face_plane.v0);
            let plane_v1 = vector3_from_point(face_plane.v1);
            let u_axis = (plane_v1 - plane_v0).normalize();
            let v_axis = plane.normal().cross(&u_axis);

            let angles = vertices
                .iter()
                .map(|vertex| {
                    let relative = vertex - plane_center;
                    relative.dot(&v_axis).atan2(relative.dot(&u_axis))
                })
                .collect::<Vec<_>>();

            let mut indices = (0..vertices.len()).collect::<Vec<_>>();
            indices.sort_unstable_by(|lhs, rhs| {
                angles[*lhs]
                    .partial_cmp(&angles[*rhs])
                    .unwrap_or(Ordering::Equal)
            });

            if winding == FaceWinding::CounterClockwise {
                indices.reverse();
            }

            indices
        })
        .collect::<Vec<_>>();
    DenseStorage::from_vec(indices).into()
}

/// Generate both winding orders from one angular sort per face.
pub fn face_indices_both(
    face_planes: &FaceTrianglePlanes,
    geo_planes: &FacePlanes,
    face_vertices: &FaceVertices,
    face_centers: &FaceCenters,
) -> (FaceIndices, FaceIndices) {
    let clockwise = face_indices(
        face_planes,
        geo_planes,
        face_vertices,
        face_centers,
        FaceWinding::Clockwise,
    );

    let counter_clockwise = clockwise
        .iter()
        .map(|indices| {
            let mut reversed = indices.clone();
            reversed.reverse();
            reversed
        })
        .collect();

    (clockwise, DenseStorage::from_vec(counter_clockwise).into())
}
