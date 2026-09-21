use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::face::FaceId;

use super::FaceIndices;

pub enum FaceTriangleIndicesTag {}

pub type FaceTriangleIndices = Usage<FaceTriangleIndicesTag, BTreeMap<FaceId, Vec<usize>>>;

/// Generate triangle indices
pub fn face_triangle_indices(face_indices: &FaceIndices) -> FaceTriangleIndices {
    face_indices
        .par_iter()
        .filter(|(_, indices)| indices.len() >= 3)
        .map(|(face_id, indices)| {
            (
                *face_id,
                (0..indices.len() - 2)
                    .flat_map(|i| [indices[0], indices[i + 1], indices[i + 2]])
                    .collect(),
            )
        })
        .collect()
}
