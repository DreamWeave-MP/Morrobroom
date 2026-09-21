use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::BrushId;
use crate::slipgate::{ConvexHull, FacePlanes, face::FaceId};

pub enum BrushHullsTag {}

pub type BrushHulls = Usage<BrushHullsTag, BTreeMap<BrushId, ConvexHull>>;

pub fn brush_hulls(
    brush_planes: &BTreeMap<BrushId, Vec<FaceId>>,
    geo_planes: &FacePlanes,
) -> BrushHulls {
    brush_planes
        .par_iter()
        .map(|(brush_id, plane_ids)| {
            let planes = plane_ids
                .iter()
                .map(|plane_id| geo_planes[plane_id])
                .collect::<Vec<_>>();
            (*brush_id, planes.into())
        })
        .collect()
}
