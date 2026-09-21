//! Lookup table from LineId to the FaceIds it connects to
use std::collections::BTreeSet;

use crate::slipgate::DenseStorage;
use crate::slipgate::face::{FaceId, FaceVertices};
use usage::Usage;

use super::{LineFaces, LineId, Lines, point_in_line};
pub enum LineFaceConnectionsTag {}
pub type LineFaceConnections =
    Usage<LineFaceConnectionsTag, DenseStorage<LineId, BTreeSet<FaceId>>>;

pub fn line_face_connections(
    lines: &Lines,
    line_faces: &LineFaces,
    face_vertices: &FaceVertices,
) -> LineFaceConnections {
    let mut line_face_connections = vec![BTreeSet::new(); lines.len()];

    // Iterate over LHS lines
    for (lhs_index, lhs) in lines.iter().enumerate() {
        let lhs_id = LineId(lhs_index);
        // Fetch LHS parent face
        let lhs_face = &line_faces[lhs_id];

        // Add LHS parent face to connections
        line_face_connections[lhs_id.0].insert(*lhs_face);

        // Fetch LHS vertices
        let lhs_v0 = &face_vertices[*lhs_face][lhs.i0];
        let lhs_v1 = &face_vertices[*lhs_face][lhs.i1];

        // Iterate over RHS lines
        for (rhs_index, rhs) in lines.iter().enumerate() {
            let rhs_id = LineId(rhs_index);
            // Skip comparing against self
            if lhs_id == rhs_id {
                continue;
            }

            // Fetch RHS parent face
            let rhs_face = &line_faces[rhs_id];

            // Add RHS parent face to connections
            line_face_connections[rhs_id.0].insert(*rhs_face);

            // Fetch RHS vertices
            let rhs_v0 = &face_vertices[*rhs_face][rhs.i0];
            let rhs_v1 = &face_vertices[*rhs_face][rhs.i1];

            // If the lines are equal, the LHS line connects to the RHS face and vice-versa
            let lhs_contain_rhs =
                point_in_line(rhs_v0, lhs_v0, lhs_v1) && point_in_line(rhs_v1, lhs_v0, lhs_v1);

            let rhs_contain_lhs =
                point_in_line(lhs_v0, rhs_v0, rhs_v1) && point_in_line(lhs_v1, rhs_v0, rhs_v1);

            //let eq = line_eq(lhs_v0, lhs_v1, rhs_v0, rhs_v1);
            let eq = lhs_contain_rhs || rhs_contain_lhs;

            if eq {
                line_face_connections[lhs_id.0].insert(*rhs_face);

                line_face_connections[rhs_id.0].insert(*lhs_face);
            }
        }
    }

    DenseStorage::from_vec(line_face_connections).into()
}
