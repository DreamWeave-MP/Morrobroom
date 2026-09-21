use rayon::iter::ParallelIterator;
use usage::Usage;

use crate::slipgate::{DenseStorage, face::FaceId};

use super::FaceIndices;

pub enum FaceTriangleIndicesTag {}

pub type FaceTriangleIndices = Usage<FaceTriangleIndicesTag, DenseStorage<FaceId, Vec<usize>>>;

/// Generate triangle indices
#[must_use]
pub fn face_triangle_indices(face_indices: &FaceIndices) -> FaceTriangleIndices {
    let triangles = face_indices
        .par_iter()
        .map(|indices| {
            if indices.len() < 3 {
                return Vec::new();
            }
            (0..indices.len() - 2)
                .flat_map(|i| [indices[0], indices[i + 1], indices[i + 2]])
                .collect()
        })
        .collect::<Vec<_>>();
    DenseStorage::from_vec(triangles).into()
}
