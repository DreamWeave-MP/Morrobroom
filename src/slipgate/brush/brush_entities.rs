use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::slipgate::{EntityBrushes, brush::BrushId, entity::EntityId};

pub enum BrushEntitiesTag {}

pub type BrushEntities = Usage<BrushEntitiesTag, BTreeMap<BrushId, EntityId>>;

pub fn brush_entities(entity_brushes: &EntityBrushes) -> BrushEntities {
    entity_brushes
        .par_iter()
        .flat_map_iter(|(entity, brushes)| brushes.iter().map(move |brush| (*brush, *entity)))
        .collect()
}
