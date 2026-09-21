use std::collections::{BTreeMap, BTreeSet};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::{
    brush::{BrushHulls, BrushId},
    face::FaceId,
    FacePlanes, Plane3d, Vector3, EPSILON,
};

pub enum FaceVerticesTag {}
pub enum FaceVertexPlanesTag {}

pub type FaceVertices = Usage<FaceVerticesTag, BTreeMap<FaceId, Vec<Vector3>>>;
pub type FaceVertexPlanes =
    Usage<FaceVertexPlanesTag, BTreeMap<FaceId, Vec<(FaceId, FaceId, FaceId)>>>;

pub fn face_vertices(
    brush_planes: &BTreeMap<BrushId, Vec<FaceId>>,
    face_planes: &FacePlanes,
    brush_hulls: &BrushHulls,
) -> (FaceVertices, FaceVertexPlanes) {
    brush_planes
        .par_iter()
        .flat_map(|(brush_id, face_ids)| {
            let hull = &brush_hulls[brush_id];

            let face_id_set: BTreeSet<FaceId> = face_ids.iter().copied().collect();
            let plane_iter = face_planes.par_iter().filter_map(move |(k, v)| {
                if face_id_set.contains(k) {
                    Some((*k, *v))
                } else {
                    None
                }
            });

            // Create world vertices via triplanar intersection
            face_ids.par_iter().map(move |face_id| {
                let plane = &face_planes[face_id];
                let p0_id = *face_id;
                let p0 = *plane;
                let mut verts = plane_iter
                    .clone()
                    .flat_map(|(p1_id, p1)| {
                        plane_iter.clone().map(move |(p2_id, p2)| {
                            if let Some(position) = triplanar_intersection(&p0, &p1, &p2) {
                                if hull.contains(&position) {
                                    return Some(((p0_id, p1_id, p2_id), position));
                                }
                            }

                            None
                        })
                    })
                    .flatten()
                    .collect::<Vec<_>>();
                verts.dedup_by(|(_, lhs), (_, rhs)| lhs == rhs);
                let (vert_planes, verts) = verts.into_iter().unzip();
                ((*face_id, verts), (*face_id, vert_planes))
            })
        })
        .unzip()
}

pub fn triplanar_intersection(p0: &Plane3d, p1: &Plane3d, p2: &Plane3d) -> Option<Vector3> {
    let n0 = p0.normal();
    let n1 = p1.normal();
    let n2 = p2.normal();

    let denom = n0.cross(n1).dot(n2);

    if denom < EPSILON {
        return None;
    }

    Some(
        (n1.cross(n2) * p0.distance()
            + n2.cross(n0) * p1.distance()
            + n0.cross(n1) * p2.distance())
            / denom,
    )
}
