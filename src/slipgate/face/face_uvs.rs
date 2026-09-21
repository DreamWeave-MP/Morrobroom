use crate::slipgate::repr::{TextureOffset, TexturePlane};
use crate::slipgate::{
    DenseStorage, FaceAngles, FaceOffsets, FaceScales, FaceTextures, Plane3d, Vector2, Vector3,
    texture::{TextureId, TextureSizes},
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::BTreeMap;
use usage::Usage;

use super::{FaceId, FacePlanes, FaceVertices};

pub enum FaceUvsTag {}

pub type FaceUvs = Usage<FaceUvsTag, DenseStorage<FaceId, Vec<Vector2>>>;

/// A prepared Valve 220 texture projection for one source face.
///
/// The source face owns the texture planes and scale/offset metadata. This
/// value is the compiled form used to evaluate UVs for emitted vertices,
/// including vertices introduced later by geometry clipping.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct TextureProjection {
    u_axis: Vector3,
    v_axis: Vector3,
    uv_scale: Vector2,
    uv_offset: Vector2,
}

impl TextureProjection {
    /// Prepare a Valve projection using the source texture planes, face scale,
    /// and resolved texture dimensions.
    pub fn from_valve(
        u_plane: TexturePlane,
        v_plane: TexturePlane,
        texture_scale: Vector2,
        texture_size: Vector2,
    ) -> Self {
        Self {
            u_axis: nalgebra::vector![u_plane.x, u_plane.y, u_plane.z],
            v_axis: nalgebra::vector![v_plane.x, v_plane.y, v_plane.z],
            uv_scale: nalgebra::vector![
                1.0 / (texture_size.x * texture_scale.x),
                1.0 / (texture_size.y * texture_scale.y),
            ],
            uv_offset: nalgebra::vector![u_plane.d / texture_size.x, v_plane.d / texture_size.y,],
        }
    }

    /// Prepare the unscaled projection axes used by tangent-space consumers.
    pub(crate) fn from_valve_axes(u_plane: TexturePlane, v_plane: TexturePlane) -> Self {
        Self {
            u_axis: nalgebra::vector![u_plane.x, u_plane.y, u_plane.z],
            v_axis: nalgebra::vector![v_plane.x, v_plane.y, v_plane.z],
            uv_scale: Vector2::new(1.0, 1.0),
            uv_offset: Vector2::new(0.0, 0.0),
        }
    }

    pub fn project(&self, position: Vector3) -> Vector2 {
        nalgebra::vector![
            self.u_axis.dot(&position) * self.uv_scale.x,
            self.v_axis.dot(&position) * self.uv_scale.y,
        ] + self.uv_offset
    }

    pub(crate) fn u_axis(&self) -> &Vector3 {
        &self.u_axis
    }

    pub(crate) fn v_axis(&self) -> &Vector3 {
        &self.v_axis
    }
}

#[allow(clippy::too_many_arguments)]
pub fn new(
    faces: &Vec<FaceId>,
    textures: &BTreeMap<TextureId, String>,
    face_textures: &FaceTextures,
    face_vertices: &FaceVertices,
    face_planes: &FacePlanes,
    face_texture_offsets: &FaceOffsets,
    face_texture_rotations: &FaceAngles,
    face_texture_scales: &FaceScales,
    texture_sizes: &TextureSizes,
) -> FaceUvs {
    let uvs = faces
        .par_iter()
        .map(|face_id| {
            let face_id = *face_id;
            let face_texture = &face_textures[face_id];
            let texture_size = texture_sizes.get(face_texture).copied().unwrap_or_else(|| {
                println!(
                    "Warning: Texture {} not found, generating UV with default size of 256x256",
                    &textures[face_texture],
                );
                (256, 256)
            });
            let face_vertices = &face_vertices[face_id];
            let face_plane = face_planes[face_id];
            let face_texture_offset = face_texture_offsets[face_id];
            let face_texture_rotation = face_texture_rotations[face_id];
            let face_texture_scale = face_texture_scales[face_id];
            let valve_projection = match face_texture_offset {
                TextureOffset::Valve { u, v } => Some(TextureProjection::from_valve(
                    u,
                    v,
                    face_texture_scale,
                    nalgebra::vector![texture_size.0 as f32, texture_size.1 as f32],
                )),
                TextureOffset::Standard { .. } => None,
            };

            face_vertices
                .iter()
                .map(|vertex| {
                    prepared_vertex_uv(
                        *vertex,
                        face_plane,
                        face_texture_offset,
                        face_texture_rotation,
                        face_texture_scale,
                        nalgebra::vector![texture_size.0 as f32, texture_size.1 as f32],
                        valve_projection.as_ref(),
                    )
                })
                .collect()
        })
        .collect::<Vec<_>>();
    DenseStorage::from_vec(uvs).into()
}

pub fn vertex_uv(
    vertex: Vector3,
    plane: Plane3d,
    texture_offset: TextureOffset,
    texture_rotation: f32,
    texture_scale: Vector2,
    texture_size: Vector2,
) -> Vector2 {
    prepared_vertex_uv(
        vertex,
        plane,
        texture_offset,
        texture_rotation,
        texture_scale,
        texture_size,
        None,
    )
}

fn prepared_vertex_uv(
    vertex: Vector3,
    plane: Plane3d,
    texture_offset: TextureOffset,
    texture_rotation: f32,
    texture_scale: Vector2,
    texture_size: Vector2,
    valve_projection: Option<&TextureProjection>,
) -> Vector2 {
    match texture_offset {
        TextureOffset::Standard { u, v } => standard_uv(
            vertex,
            plane,
            u,
            v,
            texture_rotation,
            texture_scale,
            texture_size,
        ),
        TextureOffset::Valve { u, v } => valve_projection
            .map(|projection| projection.project(vertex))
            .unwrap_or_else(|| valve_uv(vertex, u, v, texture_scale, texture_size)),
    }
}

pub fn standard_uv(
    vertex: Vector3,
    brush_plane: Plane3d,
    u_offset: f32,
    v_offset: f32,
    texture_rotation: f32,
    texture_scale: Vector2,
    texture_size: Vector2,
) -> Vector2 {
    let up_vector = Vector3::z_axis();
    let right_vector = Vector3::y_axis();
    let forward_vector = Vector3::x_axis();

    let du = brush_plane.normal().dot(&up_vector).abs();
    let dr = brush_plane.normal().dot(&right_vector).abs();
    let df = brush_plane.normal().dot(&forward_vector).abs();

    let (x, y);
    if du >= dr && du >= df {
        x = vertex.x;
        y = -vertex.y;
    } else if dr >= du && dr >= df {
        x = vertex.x;
        y = -vertex.z;
    } else if df >= du && df >= dr {
        x = vertex.y;
        y = -vertex.z;
    } else {
        panic!("Zero-length normal");
    }

    let rot = nalgebra::Rotation2::new(texture_rotation.to_radians());

    let mut uv = rot * nalgebra::vector![x, y];
    uv.x /= texture_size.x;
    uv.y /= texture_size.y;
    uv.x /= texture_scale.x;
    uv.y /= texture_scale.y;

    uv + nalgebra::vector![u_offset / texture_size.x, v_offset / texture_size.y]
}

pub fn valve_uv(
    vertex: Vector3,
    u_plane: TexturePlane,
    v_plane: TexturePlane,
    texture_scale: Vector2,
    texture_size: Vector2,
) -> Vector2 {
    TextureProjection::from_valve(u_plane, v_plane, texture_scale, texture_size).project(vertex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valve_projection_prepares_scale_and_offset_once() {
        let projection = TextureProjection::from_valve(
            TexturePlane {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                d: 4.0,
            },
            TexturePlane {
                x: -1.0,
                y: 0.0,
                z: 2.0,
                d: -8.0,
            },
            nalgebra::vector![2.0, 4.0],
            nalgebra::vector![64.0, 128.0],
        );

        let uv = projection.project(nalgebra::vector![10.0, 20.0, 30.0]);

        assert!((uv.x - 1.15625).abs() < f32::EPSILON);
        assert!((uv.y - 0.03515625).abs() < f32::EPSILON);
    }
}
