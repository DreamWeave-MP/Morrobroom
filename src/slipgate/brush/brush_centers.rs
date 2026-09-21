use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use usage::Usage;

use super::BrushId;
use crate::slipgate::{BrushFaces, DenseStorage, Vector3, face::FaceCenters};

pub enum BrushCentersTag {}

pub type BrushCenters = Usage<BrushCentersTag, DenseStorage<BrushId, Vector3>>;

// Calculate brush centers
#[must_use]
pub fn brush_centers(brush_planes: &BrushFaces, face_centers: &FaceCenters) -> BrushCenters {
    DenseStorage::from_vec(
        brush_planes
            .par_iter()
            .enumerate()
            .map(|(_, plane_ids)| {
                let mut center = Vector3::zeros();

                for plane_id in plane_ids {
                    center += face_centers[*plane_id];
                }
                center /= crate::slipgate::usize_to_f32(plane_ids.len());

                center
            })
            .collect(),
    )
    .into()
}
