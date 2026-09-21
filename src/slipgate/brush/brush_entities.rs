use usage::Usage;

use crate::slipgate::{DenseStorage, EntityBrushes, brush::BrushId, entity::EntityId};

pub enum BrushEntitiesTag {}

pub type BrushEntities = Usage<BrushEntitiesTag, DenseStorage<BrushId, EntityId>>;

#[must_use]
pub fn brush_entities(entity_brushes: &EntityBrushes) -> BrushEntities {
    DenseStorage::from_vec(
        entity_brushes
            .iter()
            .flat_map(|(entity, brushes)| brushes.iter().map(move |_| *entity))
            .collect(),
    )
    .into()
}
