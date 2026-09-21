use std::collections::{BTreeSet, VecDeque};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::{FaceCenters, FaceId, FaceNormals};
use crate::slipgate::{
    Faces,
    face::FaceLines,
    line::{LineFaceConnections, NonManifoldLines},
};

pub enum InteriorFacesTag {}

pub type InteriorFaces = Usage<InteriorFacesTag, BTreeSet<FaceId>>;

pub fn interior_faces(
    faces: &Faces,
    face_lines: &FaceLines,
    face_normals: &FaceNormals,
    face_centers: &FaceCenters,
    non_manifold_lines: &NonManifoldLines,
    line_face_connections: &LineFaceConnections,
) -> InteriorFaces {
    let mut interior_faces = BTreeSet::default();

    // Find faces whose lines are all non-manifold
    let starting_faces = faces
        .par_iter()
        .flat_map(|face| {
            let lines = face_lines.get(*face).unwrap();

            let non_manifold: usize = lines
                .iter()
                .map(|line_id| non_manifold_lines.contains(line_id) as usize)
                .sum();

            if non_manifold == lines.len() {
                Some(*face)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();

    // Initialize the traversal queue with the non-manifold faces
    let mut traversal_queue = starting_faces.iter().copied().collect::<VecDeque<_>>();

    // Traverse
    while let Some(face) = traversal_queue.pop_front() {
        if interior_faces.contains(&face) {
            continue;
        }

        interior_faces.insert(face);

        let face_normal = face_normals[face][0];

        for line_id in &face_lines[face] {
            let mut connected_faces = line_face_connections[*line_id]
                .iter()
                .filter(|candidate| **candidate != face);

            let non_manifold = non_manifold_lines.contains(line_id);
            if non_manifold {
                // Pick closest face via normal
                let mut connected_faces = connected_faces.copied().collect::<Vec<_>>();
                connected_faces.sort_unstable_by(|lhs, rhs| {
                    let lhs_face_center = face_centers[*lhs] - face_centers[face];
                    let rhs_face_center = face_centers[*rhs] - face_centers[face];

                    face_normal
                        .dot(&rhs_face_center)
                        .partial_cmp(&face_normal.dot(&lhs_face_center))
                        .unwrap()
                });
                traversal_queue.push_back(connected_faces[0]);
            } else {
                // Traverse directly
                if let Some(connected_face) = connected_faces.next() {
                    traversal_queue.push_back(*connected_face);
                }
            }
        }
    }

    interior_faces.into()
}
