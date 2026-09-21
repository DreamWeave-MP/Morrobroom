use crate::slipgate::{face::TextureProjection, repr::TextureOffset};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use usage::Usage;

use crate::slipgate::{DenseStorage, FaceOffsets, FacePlanes, Plane3d, Vector3, face::FaceId};

// TODO: Replace GeoPlane usage with custom tangent type
//       (Would storing a basis be viable? No need to conform to godot standards)

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct Basis {
    pub x: Vector3,
    pub y: Vector3,
    pub z: Vector3,
}

pub enum FaceBasesTag {}

pub type FaceBases = Usage<FaceBasesTag, DenseStorage<FaceId, Basis>>;

#[must_use]
pub fn face_bases(
    planes: &Vec<FaceId>,
    geo_planes: &FacePlanes,
    face_offsets: &FaceOffsets,
) -> FaceBases {
    let bases = planes
        .par_iter()
        .map(|plane_id| face_basis(&geo_planes[*plane_id], &face_offsets[*plane_id]))
        .collect::<Vec<_>>();
    DenseStorage::from_vec(bases).into()
}

fn face_basis(geo_plane: &Plane3d, offset: &TextureOffset) -> Basis {
    match &offset {
        TextureOffset::Standard { .. } => standard_basis(geo_plane),
        TextureOffset::Valve { .. } => valve_basis(geo_plane, offset),
    }
}

fn standard_basis(plane: &Plane3d) -> Basis {
    let up_vector: &Vector3 = &Vector3::z_axis();
    let right_vector: &Vector3 = &Vector3::y_axis();
    let forward_vector: &Vector3 = &Vector3::x_axis();

    let normal = plane.normal();

    let du = normal.dot(up_vector);
    let dr = normal.dot(right_vector);
    let df = normal.dot(forward_vector);

    let up_abs = du.abs();
    let right_abs = dr.abs();
    let forward_abs = df.abs();

    let up_sign = du.signum();
    let right_sign = dr.signum();
    let forward_sign = df.signum();

    if up_abs >= right_abs && up_abs >= forward_abs {
        let z = *plane.normal() * up_sign;
        let x = z.cross(forward_vector).normalize();
        let y = z.cross(right_vector).normalize();
        Basis { x, y, z }
    } else if right_abs >= up_abs && right_abs >= forward_abs {
        let z = *plane.normal() * right_sign;
        let x = z.cross(up_vector).normalize();
        let y = z.cross(forward_vector).normalize();
        Basis { x, y, z }
    } else if forward_abs >= up_abs && forward_abs >= right_abs {
        let z = *plane.normal() * forward_sign;
        let x = z.cross(up_vector).normalize();
        let y = z.cross(right_vector).normalize();
        Basis { x, y, z }
    } else {
        panic!("Failed to generate basis")
    }
}

fn valve_basis(plane: &Plane3d, texture_offset: &TextureOffset) -> Basis {
    if let TextureOffset::Valve { u, v } = &texture_offset {
        let projection = TextureProjection::from_valve_axes(*u, *v);
        Basis {
            x: *projection.u_axis(),
            y: *projection.v_axis(),
            z: *plane.normal(),
        }
    } else {
        panic!("Not a valve UV");
    }
}
