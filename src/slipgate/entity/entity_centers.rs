use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::EntityId;
use crate::slipgate::{
    Vector3,
    brush::{BrushCenters, BrushId},
};

pub enum EntityCentersTag {}

pub type EntityCenters = Usage<EntityCentersTag, BTreeMap<EntityId, Vector3>>;

// Calculate entity centers
#[must_use]
pub fn entity_centers(
    entity_brushes: &BTreeMap<EntityId, Vec<BrushId>>,
    brush_centers: &BrushCenters,
) -> EntityCenters {
    entity_brushes
        .par_iter()
        .map(|(entity_id, brush_ids)| {
            let center: Vector3 = brush_ids
                .iter()
                .map(|brush_id| brush_centers[*brush_id])
                .sum();

            let center = center / crate::slipgate::usize_to_f32(brush_ids.len());

            (*entity_id, center)
        })
        .collect()
}
