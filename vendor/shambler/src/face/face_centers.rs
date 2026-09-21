use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::{FaceId, FaceVertices};
use crate::Vector3;

pub enum FaceCentersTag {}

pub type FaceCenters = Usage<FaceCentersTag, BTreeMap<FaceId, Vector3>>;

// Calculate face centers
pub fn face_centers(face_vertices: &FaceVertices) -> FaceCenters {
    face_vertices
        .par_iter()
        .map(|(face_id, vertices)| {
            let mut center = Vector3::zeros();
            for world_vertex in vertices {
                center += world_vertex;
            }
            center /= vertices.len() as f32;
            (*face_id, center)
        })
        .collect()
}
