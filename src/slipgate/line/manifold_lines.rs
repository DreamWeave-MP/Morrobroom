use std::collections::BTreeSet;

use usage::{AsUsage, Usage};

use super::{LineFaceConnections, LineId};

pub enum ManifoldTag {}
pub type ManifoldLines = Usage<ManifoldTag, BTreeSet<LineId>>;

pub enum NonManifoldTag {}
pub type NonManifoldLines = Usage<NonManifoldTag, BTreeSet<LineId>>;

pub fn manifold_lines(
    line_face_connections: &LineFaceConnections,
) -> (ManifoldLines, NonManifoldLines) {
    let mut manifold_lines = BTreeSet::default();
    let mut non_manifold_lines = BTreeSet::default();
    for (line, faces) in line_face_connections.iter() {
        if faces.len() > 2 {
            non_manifold_lines.insert(*line);
        } else {
            manifold_lines.insert(*line);
        }
    }
    (
        ManifoldTag::as_usage(manifold_lines),
        NonManifoldTag::as_usage(non_manifold_lines),
    )
}
