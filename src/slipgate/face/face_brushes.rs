use usage::Usage;

use crate::slipgate::{BrushFaces, DenseStorage, brush::BrushId};

use super::FaceId;

pub enum FaceBrushesTag {}

pub type FaceBrushes = Usage<FaceBrushesTag, DenseStorage<FaceId, BrushId>>;

#[must_use]
pub fn face_brushes(brush_faces: &BrushFaces) -> FaceBrushes {
    DenseStorage::from_vec(
        brush_faces
            .iter()
            .enumerate()
            .flat_map(|(brush_index, faces)| {
                let brush = BrushId(brush_index);
                faces.iter().map(move |_| brush)
            })
            .collect(),
    )
    .into()
}
