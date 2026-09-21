use rayon::iter::ParallelIterator;
use usage::Usage;

use crate::slipgate::{DenseStorage, Plane3d};

use super::FaceId;
use crate::slipgate::FaceTrianglePlanes;

pub enum FacePlanesTag {}

pub type FacePlanes = Usage<FacePlanesTag, DenseStorage<FaceId, Plane3d>>;

pub fn face_planes(face_triangle_planes: &FaceTrianglePlanes) -> FacePlanes {
    let planes: Vec<_> = face_triangle_planes.par_iter().map(Plane3d::from).collect();
    DenseStorage::from_vec(planes).into()
}
