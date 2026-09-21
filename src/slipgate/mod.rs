//! Map parsing and brush geometry owned by Morrobroom.

pub mod brush;
pub mod csg;
pub mod entity;
pub mod face;
#[cfg(test)]
mod integration_tests;
pub mod line;
pub mod map_geometry;
pub mod parser;
pub mod repr;
pub mod texture;

mod convex_hull;
mod dense_storage;
mod geo_map;
mod plane_3d;
#[cfg(test)]
mod unit_test_data;

pub use convex_hull::*;
pub use dense_storage::*;
pub use geo_map::*;
pub use plane_3d::*;

use repr::{Point, TexturePlane};

use crate::slipgate::face::FacePlanes;

const EPSILON: f32 = 0.001;

pub type Vector2 = nalgebra::Vector2<f32>;
pub type Vector3 = nalgebra::Vector3<f32>;

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "Geometry cardinalities are normalized by the f32 math layer."
)]
pub(crate) fn usize_to_f32(value: usize) -> f32 {
    value as f32
}

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "Texture dimensions are consumed by the f32 UV math layer."
)]
pub(crate) fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "The geometry pipeline stores vertex coordinates as f32 by contract."
)]
pub(crate) fn f64_to_f32(value: f64) -> f32 {
    let converted = value as f32;
    assert!(converted.is_finite(), "vertex coordinate must be finite");
    converted
}

#[must_use]
pub fn vector3_from_point(point: Point) -> Vector3 {
    nalgebra::vector![point.x, point.y, point.z]
}

#[must_use]
pub fn vector3_from_texture_plane(plane: &TexturePlane) -> Vector3 {
    nalgebra::vector![plane.x, plane.y, plane.z]
}

#[cfg(test)]
mod tests {
    use super::repr::Map;

    #[test]
    fn parser_and_geometry_share_the_slipgate_boundary() {
        let map = "{\n\"classname\" \"worldspawn\"\n}\n"
            .parse::<Map>()
            .expect("minimal map should parse");
        assert_eq!(map.len(), 1);
    }
}
