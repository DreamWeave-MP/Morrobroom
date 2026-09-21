use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::Vector3;
use std::collections::BTreeMap;

use super::{FaceId, FacePlanes, FaceVertexPlanes, FaceVertices};

pub enum FaceNormalsTag {}

pub type FaceNormals = Usage<FaceNormalsTag, BTreeMap<FaceId, Vec<Vector3>>>;

/// Copy normals from face planes
pub fn normals_flat(face_vertices: &FaceVertices, face_planes: &FacePlanes) -> FaceNormals {
    face_vertices
        .par_iter()
        .map(|(face_id, vertices)| {
            let face_plane = &face_planes[face_id];

            (
                *face_id,
                vertices.par_iter().map(|_| *face_plane.normal()).collect(),
            )
        })
        .collect()
}

/// Average normals from vertex planes with each plane contributing equally
///
/// Good for spherical objects
pub fn normals_phong_averaged(
    face_vertex_planes: &FaceVertexPlanes,
    face_planes: &FacePlanes,
) -> FaceNormals {
    face_vertex_planes
        .par_iter()
        .map(|(face_id, vertex_planes)| {
            (
                *face_id,
                vertex_planes
                    .par_iter()
                    .map(|(p0, p1, p2)| {
                        let p0 = &face_planes[p0];
                        let p1 = &face_planes[p1];
                        let p2 = &face_planes[p2];
                        (p0.normal() + p1.normal() + p2.normal()).normalize()
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Average normals from vertex planes using an angular threshold given in degrees
///
/// Good for cylindrical objects
pub fn normals_phong_threshold(
    face_vertex_planes: &FaceVertexPlanes,
    face_planes: &FacePlanes,
    threshold: f32,
) -> FaceNormals {
    face_vertex_planes
        .par_iter()
        .map(|(face_id, vertex_planes)| {
            (
                *face_id,
                vertex_planes
                    .par_iter()
                    .map(|(p0, p1, p2)| {
                        let p0 = &face_planes[p0];
                        let p1 = &face_planes[p1];
                        let p2 = &face_planes[p2];

                        const ONE_DEGREE: f32 = 0.017_453_3;

                        let threshold = ((threshold + 0.01) * ONE_DEGREE).cos();
                        let mut normal = *p0.normal();
                        if p0.normal().dot(p1.normal()) > threshold {
                            normal += p1.normal()
                        }
                        if p0.normal().dot(p2.normal()) > threshold {
                            normal += p2.normal()
                        }
                        normal.normalize()
                    })
                    .collect(),
            )
        })
        .collect()
}
