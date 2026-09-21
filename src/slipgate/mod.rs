//! Map parsing and brush geometry owned by Morrobroom.

pub mod brush;
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

pub fn vector3_from_point(point: Point) -> Vector3 {
    nalgebra::vector![point.x, point.y, point.z]
}

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
