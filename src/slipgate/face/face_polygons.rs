use usage::Usage;

use crate::slipgate::{DenseStorage, Vector3};

use super::{FaceId, FaceIndices, FaceVertices};

pub enum FacePolygonsTag {}

/// Canonically ordered source polygon vertices for each face.
pub type FacePolygons = Usage<FacePolygonsTag, DenseStorage<FaceId, Vec<Vector3>>>;

/// Materialize the ordered face polygon before triangulation.
///
/// Face vertices are generated from plane intersections and are therefore not
/// inherently ordered. `FaceIndices` is the canonical angular ordering; this
/// stage turns that ordering into the source polygon consumed by CSG. Triangle
/// topology is intentionally not involved.
#[must_use]
pub fn face_polygons(face_indices: &FaceIndices, face_vertices: &FaceVertices) -> FacePolygons {
    let polygons = face_indices
        .iter()
        .enumerate()
        .map(|(face_index, indices)| {
            let vertices = &face_vertices[FaceId(face_index)];
            indices.iter().map(|index| vertices[*index]).collect()
        })
        .collect();

    DenseStorage::from_vec(polygons).into()
}
