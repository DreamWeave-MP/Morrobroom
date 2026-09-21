use std::collections::BTreeMap;

use crate::slipgate::repr::TrianglePlane;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::slipgate::{DenseStorage, Plane3d};

use super::FaceId;

pub enum FacePlanesTag {}

pub type FacePlanes = Usage<FacePlanesTag, DenseStorage<FaceId, Plane3d>>;

pub fn face_planes(face_triangle_planes: &BTreeMap<FaceId, TrianglePlane>) -> FacePlanes {
    let mut planes: Vec<_> = face_triangle_planes
        .par_iter()
        .map(|(face_id, face_plane)| (*face_id, Plane3d::from(face_plane)))
        .collect();
    planes.sort_unstable_by_key(|(face_id, _)| face_id.0);
    DenseStorage::from_vec(planes.into_iter().map(|(_, plane)| plane).collect()).into()
}
