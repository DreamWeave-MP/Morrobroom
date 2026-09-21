use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::EntityId;
use crate::{
    brush::{BrushCenters, BrushId},
    Vector3,
};

pub enum EntityCentersTag {}

pub type EntityCenters = Usage<EntityCentersTag, BTreeMap<EntityId, Vector3>>;

// Calculate entity centers
pub fn entity_centers(
    entity_brushes: &BTreeMap<EntityId, Vec<BrushId>>,
    brush_centers: &BrushCenters,
) -> EntityCenters {
    entity_brushes
        .par_iter()
        .map(|(entity_id, brush_ids)| {
            let center: Vector3 = brush_ids
                .par_iter()
                .map(|brush_id| brush_centers[brush_id])
                .sum();

            let center = center / brush_ids.len() as f32;

            (*entity_id, center)
        })
        .collect()
}
