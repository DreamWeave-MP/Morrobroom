use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use super::{FaceBases, FaceId, FacePlanes, FaceVertices};
use crate::slipgate::{face::FaceLines, line::Lines, EPSILON};

pub enum FaceFaceContainmentTag {}

pub type FaceFaceContainment = Usage<FaceFaceContainmentTag, BTreeMap<FaceId, Vec<FaceId>>>;

// Find contained faces
pub fn face_face_containment(
    faces: &Vec<FaceId>,
    lines: &Lines,
    face_planes: &FacePlanes,
    face_bases: &FaceBases,
    face_vertices: &FaceVertices,
    face_lines: &FaceLines,
) -> FaceFaceContainment {
    let iter = faces
        .par_iter()
        .flat_map(|lhs_id| {
            let lhs_verts = &face_vertices[lhs_id];
            let lhs_plane = &face_planes[lhs_id];
            let lhs_basis = &face_bases[lhs_id];

            faces
                .par_iter()
                .flat_map(move |rhs_id| {
                    let rhs_verts = &face_vertices[rhs_id];
                    let rhs_plane = &face_planes[rhs_id];

                    // Skip comparing with self
                    if lhs_id == rhs_id {
                        return None;
                    }

                    // Skip faces that don't lie on the same plane
                    if !lhs_plane.opposes(rhs_plane) {
                        return None;
                    }

                    let contained = &face_lines[lhs_id].par_iter().all(|line_id| {
                        let line = lines[line_id];

                        let v0 = lhs_verts[line.i0];
                        let v1 = lhs_verts[line.i1];

                        let vd0 = nalgebra::vector![v0.dot(&lhs_basis.x), v0.dot(&lhs_basis.y)];
                        let vd1 = nalgebra::vector![v1.dot(&lhs_basis.x), v1.dot(&lhs_basis.y)];

                        let u = (vd1 - vd0).normalize();
                        let v = nalgebra::vector![-u.y, u.x];

                        rhs_verts.par_iter().all(|vert| {
                            let vert =
                                nalgebra::vector![vert.dot(&lhs_basis.x), vert.dot(&lhs_basis.y)];
                            vert.dot(&v) > vd0.dot(&v) + EPSILON
                        })
                    });

                    if !contained {
                        return None;
                    }

                    Some((*lhs_id, *rhs_id))
                })
        });

    let pairs: Vec<(FaceId, FaceId)> = iter.collect();
    let mut map: BTreeMap<FaceId, Vec<FaceId>> = BTreeMap::default();
    for (lhs_id, rhs_id) in pairs {
        map.entry(lhs_id).or_default().push(rhs_id);
    }
    map.into()
}
