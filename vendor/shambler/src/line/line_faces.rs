//! Lookup table from LineId to its parent FaceId
use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::face::{FaceId, FaceLines};

use super::LineId;

pub enum LineFacesTag {}
pub type LineFaces = Usage<LineFacesTag, BTreeMap<LineId, FaceId>>;

pub fn line_faces(face_lines: &FaceLines) -> LineFaces {
    face_lines
        .par_iter()
        .flat_map(|(face, lines)| lines.par_iter().map(move |line| (*line, *face)))
        .collect()
}
