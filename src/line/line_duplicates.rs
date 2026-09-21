use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::BTreeSet;
use std::sync::Arc;
use usage::Usage;

use super::{line_eq, LineId, Lines};

use crate::{
    face::{FaceDuplicates, FaceId, FaceLines, FaceVertices},
    BrushFaces, Brushes,
};

pub enum LineDuplicatesTag {}
pub type LineDuplicates = Usage<LineDuplicatesTag, BTreeSet<(LineId, LineId)>>;

pub fn line_duplicates(
    brushes: &Brushes,
    lines: &Lines,
    brush_faces: &BrushFaces,
    face_duplicates: &FaceDuplicates,
    face_vertices: &FaceVertices,
    face_lines: &FaceLines,
) -> LineDuplicates {
    let duplicate_face_ids: Arc<BTreeSet<FaceId>> = Arc::new(
        face_duplicates.iter().map(|(a, _)| *a).collect()
    );

    brushes
        .par_iter()
        .flat_map(|brush_a| {
            let faces_a = &brush_faces[brush_a];
            // Each invocation of this Fn closure gets its own clones.
            let dfi_filter = Arc::clone(&duplicate_face_ids);
            let dfi_face = Arc::clone(&duplicate_face_ids);

            // Iterate over LHS brush faces
            faces_a
                .par_iter()
                .filter(move |f| !dfi_filter.contains(*f))
                .flat_map(move |face_a| {
                    // Fetch LHS vertex and line data
                    let verts_a = &face_vertices[face_a];
                    let lines_a = &face_lines[face_a];
                    let dfi_line = Arc::clone(&dfi_face);

                    // Iterate over LHS face lines
                    lines_a.par_iter().flat_map(move |line_id_a| {
                        // Fetch LHS line indices
                        let line_a = lines[line_id_a];

                        // Fetch LHS line vertices
                        let v0_a = &verts_a[line_a.i0];
                        let v1_a = &verts_a[line_a.i1];
                        let dfi_brush = Arc::clone(&dfi_line);

                        // Iterate over brushes again to compare
                        brushes
                            .par_iter()
                            .flat_map(|brush_b| {
                                // Skip comparing with self
                                if brush_a == brush_b {
                                    return None;
                                }

                                // Fetch each brush's faces
                                let faces_b = &brush_faces[brush_b];
                                let dfi_rhs = Arc::clone(&dfi_brush);

                                // Iterate over RHS brush faces
                                Some(
                                    faces_b
                                        .par_iter()
                                        .filter(move |f| !dfi_rhs.contains(*f))
                                        .flat_map(|face_b| {
                                            // Fetch RHS vertex and line data
                                            let verts_b = &face_vertices[face_b];
                                            let lines_b = &face_lines[face_b];

                                            // Iterate over RHS face lines
                                            lines_b
                                                .par_iter()
                                                .flat_map(move |line_id_b| {
                                                    // Fetch RHS line indices
                                                    let line_b = lines[line_id_b];

                                                    // Fetch RHS line vertices
                                                    let v0_b = &verts_b[line_b.i0];
                                                    let v1_b = &verts_b[line_b.i1];

                                                    // If the lines are equivalent, add them to the set
                                                    if line_eq(v0_a, v1_a, v0_b, v1_b) {
                                                        Some([
                                                            (*line_id_a, *line_id_b),
                                                            (*line_id_b, *line_id_a),
                                                        ])
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .flatten()
                                        }),
                                )
                            })
                            .flatten()
                            .collect::<BTreeSet<_>>()
                    })
                })
        })
        .collect()
}
