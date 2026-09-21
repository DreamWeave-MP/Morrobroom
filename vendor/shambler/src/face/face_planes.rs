use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use shalrath::repr::TrianglePlane;
use usage::Usage;

use crate::Plane3d;

use super::FaceId;

pub enum FacePlanesTag {}

pub type FacePlanes = Usage<FacePlanesTag, BTreeMap<FaceId, Plane3d>>;

pub fn face_planes(face_triangle_planes: &BTreeMap<FaceId, TrianglePlane>) -> FacePlanes {
    face_triangle_planes
        .par_iter()
        .map(|(plane_id, face_plane)| (*plane_id, Plane3d::from(face_plane)))
        .collect()
}
