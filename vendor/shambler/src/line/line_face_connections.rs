//! Lookup table from LineId to the FaceIds it connects to
use std::collections::{BTreeMap, BTreeSet};

use crate::face::{FaceId, FaceVertices};
use usage::{AsUsage, Usage};

use super::{point_in_line, LineFaces, LineId, Lines};
pub enum LineFaceConnectionsTag {}
pub type LineFaceConnections = Usage<LineFaceConnectionsTag, BTreeMap<LineId, BTreeSet<FaceId>>>;

pub fn line_face_connections(
    lines: &Lines,
    line_faces: &LineFaces,
    face_vertices: &FaceVertices,
) -> LineFaceConnections {
    let mut line_face_connections = BTreeMap::<LineId, BTreeSet<FaceId>>::default();

    // Iterate over LHS lines
    for (lhs_id, lhs) in lines.iter() {
        // Fetch LHS parent face
        let lhs_face = &line_faces[lhs_id];

        // Add LHS parent face to connections
        line_face_connections
            .entry(*lhs_id)
            .or_default()
            .insert(*lhs_face);

        // Fetch LHS vertices
        let lhs_v0 = &face_vertices[lhs_face][lhs.i0];
        let lhs_v1 = &face_vertices[lhs_face][lhs.i1];

        // Iterate over RHS lines
        for (rhs_id, rhs) in lines.iter() {
            // Skip comparing against self
            if lhs_id == rhs_id {
                continue;
            }

            // Fetch RHS parent face
            let rhs_face = &line_faces[rhs_id];

            // Add RHS parent face to connections
            line_face_connections
                .entry(*rhs_id)
                .or_default()
                .insert(*rhs_face);

            // Fetch RHS vertices
            let rhs_v0 = &face_vertices[rhs_face][rhs.i0];
            let rhs_v1 = &face_vertices[rhs_face][rhs.i1];

            // If the lines are equal, the LHS line connects to the RHS face and vice-versa
            let lhs_contain_rhs =
                point_in_line(rhs_v0, lhs_v0, lhs_v1) && point_in_line(rhs_v1, lhs_v0, lhs_v1);

            let rhs_contain_lhs =
                point_in_line(lhs_v0, rhs_v0, rhs_v1) && point_in_line(lhs_v1, rhs_v0, rhs_v1);

            //let eq = line_eq(lhs_v0, lhs_v1, rhs_v0, rhs_v1);
            let eq = lhs_contain_rhs || rhs_contain_lhs;

            if eq {
                line_face_connections
                    .entry(*lhs_id)
                    .or_default()
                    .insert(*rhs_face);

                line_face_connections
                    .entry(*rhs_id)
                    .or_default()
                    .insert(*lhs_face);
            }
        }
    }

    LineFaceConnectionsTag::as_usage(line_face_connections)
}
