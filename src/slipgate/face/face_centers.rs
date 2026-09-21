use rayon::iter::ParallelIterator;
use usage::Usage;

use super::{FaceId, FaceVertices};
use crate::slipgate::{DenseStorage, Vector3};

pub enum FaceCentersTag {}

pub type FaceCenters = Usage<FaceCentersTag, DenseStorage<FaceId, Vector3>>;

// Calculate face centers
#[must_use]
pub fn face_centers(face_vertices: &FaceVertices) -> FaceCenters {
    let centers = face_vertices
        .par_iter()
        .map(|vertices| {
            let mut center = Vector3::zeros();
            for world_vertex in vertices {
                center += world_vertex;
            }
            center /= crate::slipgate::usize_to_f32(vertices.len());
            center
        })
        .collect();
    DenseStorage::from_vec(centers).into()
}
