use std::collections::BTreeMap;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use shalrath::repr::TextureOffset;
use usage::Usage;

use crate::{face::FaceId, vector3_from_texture_plane, FacePlanes, Plane3d, Vector3};

// TODO: Replace GeoPlane usage with custom tangent type
//       (Would storing a basis be viable? No need to conform to godot standards)

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct Basis {
    pub x: Vector3,
    pub y: Vector3,
    pub z: Vector3,
}

pub enum FaceBasesTag {}

pub type FaceBases = Usage<FaceBasesTag, BTreeMap<FaceId, Basis>>;

pub fn face_bases(
    planes: &Vec<FaceId>,
    geo_planes: &FacePlanes,
    face_offsets: &BTreeMap<FaceId, TextureOffset>,
) -> FaceBases {
    planes
        .par_iter()
        .map(|plane_id| {
            (
                *plane_id,
                face_basis(
                    &geo_planes[plane_id],
                    &face_offsets[plane_id],
                ),
            )
        })
        .collect()
}

fn face_basis(geo_plane: &Plane3d, offset: &TextureOffset) -> Basis {
    match &offset {
        shalrath::repr::TextureOffset::Standard { .. } => standard_basis(geo_plane),
        shalrath::repr::TextureOffset::Valve { .. } => valve_basis(geo_plane, offset),
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

    let du_abs = du.abs();
    let dr_abs = dr.abs();
    let df_abs = df.abs();

    let du_sign = du.signum();
    let dr_sign = dr.signum();
    let df_sign = df.signum();

    if du_abs >= dr_abs && du_abs >= df_abs {
        let z = *plane.normal() * du_sign;
        let x = z.cross(forward_vector).normalize();
        let y = z.cross(right_vector).normalize();
        Basis { x, y, z }
    } else if dr_abs >= du_abs && dr_abs >= df_abs {
        let z = *plane.normal() * dr_sign;
        let x = z.cross(up_vector).normalize();
        let y = z.cross(forward_vector).normalize();
        Basis { x, y, z }
    } else if df_abs >= du_abs && df_abs >= dr_abs {
        let z = *plane.normal() * df_sign;
        let x = z.cross(up_vector).normalize();
        let y = z.cross(right_vector).normalize();
        Basis { x, y, z }
    } else {
        panic!("Failed to generate basis")
    }
}

fn valve_basis(plane: &Plane3d, texture_offset: &TextureOffset) -> Basis {
    if let shalrath::repr::TextureOffset::Valve { u, v } = &texture_offset {
        let u = vector3_from_texture_plane(u);
        let v = vector3_from_texture_plane(v);
        Basis {
            x: u,
            y: v,
            z: *plane.normal(),
        }
    } else {
        panic!("Not a valve UV");
    }
}
