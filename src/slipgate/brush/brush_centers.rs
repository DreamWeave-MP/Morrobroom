use std::collections::BTreeMap;

use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use usage::Usage;

use super::BrushId;
use crate::slipgate::{BrushFaces, Vector3, face::FaceCenters};

pub enum BrushCentersTag {}

pub type BrushCenters = Usage<BrushCentersTag, BTreeMap<BrushId, Vector3>>;

// Calculate brush centers
pub fn brush_centers(brush_planes: &BrushFaces, face_centers: &FaceCenters) -> BrushCenters {
    brush_planes
        .par_iter()
        .enumerate()
        .map(|(brush_index, plane_ids)| {
            let brush_id = BrushId(brush_index);
            let mut center = Vector3::zeros();

            for plane_id in plane_ids {
                center += face_centers[*plane_id];
            }
            center /= plane_ids.len() as f32;

            (brush_id, center)
        })
        .collect()
}
