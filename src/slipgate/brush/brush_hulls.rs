use rayon::iter::ParallelIterator;
use usage::Usage;

use super::BrushId;
use crate::slipgate::{BrushFaces, ConvexHull, DenseStorage, FacePlanes};

pub enum BrushHullsTag {}

pub type BrushHulls = Usage<BrushHullsTag, DenseStorage<BrushId, ConvexHull>>;

#[must_use]
pub fn brush_hulls(brush_planes: &BrushFaces, geo_planes: &FacePlanes) -> BrushHulls {
    let hulls = brush_planes
        .par_iter()
        .map(|plane_ids| {
            let planes = plane_ids
                .iter()
                .map(|plane_id| geo_planes[*plane_id])
                .collect::<Vec<_>>();
            planes.into()
        })
        .collect();
    DenseStorage::from_vec(hulls).into()
}
