pub mod brush;
pub mod entity;
pub mod face;
pub mod texture;
pub mod line;
pub mod map_geometry;

mod convex_hull;
mod geo_map;
mod plane_3d;

pub use convex_hull::*;
pub use geo_map::*;
pub use plane_3d::*;

pub use shalrath;

use shalrath::repr::{Point, TexturePlane};

use crate::face::FacePlanes;

const EPSILON: f32 = 0.001;

pub type Vector2 = nalgebra::Vector2<f32>;
pub type Vector3 = nalgebra::Vector3<f32>;

pub fn vector3_from_point(point: Point) -> Vector3 {
    nalgebra::vector![point.x, point.y, point.z]
}

pub fn vector3_from_texture_plane(plane: &TexturePlane) -> Vector3 {
    nalgebra::vector![plane.x, plane.y, plane.z]
}
