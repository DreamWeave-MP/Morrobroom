use std::collections::BTreeSet;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::{BrushHulls, BrushId};
use crate::slipgate::{
    BrushFaces, DenseStorage,
    face::{FaceId, FaceVertices},
};

pub enum BrushFaceContainmentTag {}

pub type BrushFaceContainment = Usage<BrushFaceContainmentTag, DenseStorage<BrushId, Vec<FaceId>>>;

// Find contained faces
pub fn brush_face_containment(
    brushes: &Vec<BrushId>,
    faces: &Vec<FaceId>,
    brush_faces: &BrushFaces,
    brush_hulls: &BrushHulls,
    face_vertices: &FaceVertices,
) -> BrushFaceContainment {
    let contained_faces = brushes
        .par_iter()
        .map(|brush_id| {
            let brush_faces_set: BTreeSet<FaceId> =
                brush_faces[*brush_id].iter().copied().collect();
            let brush_hull = &brush_hulls[*brush_id];

            faces
                .iter()
                .filter_map(|face_id| {
                    // Skip checking own vertices
                    if brush_faces_set.contains(face_id) {
                        return None;
                    }

                    let face_verts = &face_vertices[*face_id];

                    let contained = face_verts.iter().all(|vertex| brush_hull.contains(vertex));

                    if !contained {
                        return None;
                    }

                    Some(*face_id)
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    DenseStorage::from_vec(contained_faces).into()
}
