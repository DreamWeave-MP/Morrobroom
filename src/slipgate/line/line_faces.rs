//! Lookup table from `LineId` to its parent `FaceId`
use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use usage::Usage;

use crate::slipgate::{
    DenseStorage,
    face::{FaceId, FaceLines},
};

use super::LineId;

pub enum LineFacesTag {}
pub type LineFaces = Usage<LineFacesTag, DenseStorage<LineId, FaceId>>;

pub fn line_faces(face_lines: &FaceLines) -> LineFaces {
    let line_count = face_lines.as_slice().iter().map(Vec::len).sum();
    let mut line_faces = vec![FaceId::default(); line_count];
    let mappings: Vec<_> = face_lines
        .par_iter()
        .enumerate()
        .flat_map_iter(|(face_index, lines)| {
            let face = FaceId(face_index);
            lines.iter().map(move |line| (line.0, face))
        })
        .collect();
    for (line_index, face) in mappings {
        line_faces[line_index] = face;
    }
    DenseStorage::from_pairs(
        line_faces
            .into_iter()
            .enumerate()
            .map(|(line_index, face)| (LineId(line_index), face)),
    )
    .into()
}
