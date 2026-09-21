use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::BrushId;
use crate::slipgate::{ConvexHull, DenseStorage, FacePlanes, face::FaceId};

pub enum BrushHullsTag {}

pub type BrushHulls = Usage<BrushHullsTag, DenseStorage<BrushId, ConvexHull>>;

pub fn brush_hulls(
    brush_planes: &BTreeMap<BrushId, Vec<FaceId>>,
    geo_planes: &FacePlanes,
) -> BrushHulls {
    let mut hulls: Vec<_> = brush_planes
        .par_iter()
        .map(|(brush_id, plane_ids)| {
            let planes = plane_ids
                .iter()
                .map(|plane_id| geo_planes[*plane_id])
                .collect::<Vec<_>>();
            (*brush_id, planes.into())
        })
        .collect();
    hulls.sort_unstable_by_key(|(brush_id, _)| brush_id.0);
    DenseStorage::from_vec(hulls.into_iter().map(|(_, hull)| hull).collect()).into()
}
