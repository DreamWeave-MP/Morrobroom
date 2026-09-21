use std::collections::BTreeMap;

use usage::Usage;

use crate::{brush::BrushId, BrushFaces};

use super::FaceId;

pub enum FaceBrushesTag {}

pub type FaceBrushes = Usage<FaceBrushesTag, BTreeMap<FaceId, BrushId>>;

pub fn face_brushes(brush_faces: &BrushFaces) -> FaceBrushes {
    brush_faces
        .iter()
        .flat_map(|(brush, faces)| faces.iter().map(move |face| (*face, *brush)))
        .collect()
}
