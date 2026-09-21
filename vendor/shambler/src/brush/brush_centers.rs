use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::BrushId;
use crate::{
    face::{FaceCenters, FaceId},
    Vector3,
};

pub enum BrushCentersTag {}

pub type BrushCenters = Usage<BrushCentersTag, BTreeMap<BrushId, Vector3>>;

// Calculate brush centers
pub fn brush_centers(brush_planes: &BTreeMap<BrushId, Vec<FaceId>>, face_centers: &FaceCenters) -> BrushCenters {
    brush_planes
        .par_iter()
        .map(|(brush_id, plane_ids)| {
            let mut center = Vector3::zeros();

            for plane_id in plane_ids {
                center += face_centers[plane_id];
            }
            center /= plane_ids.len() as f32;

            (*brush_id, center)
        })
        .collect()
}

