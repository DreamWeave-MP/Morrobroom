use crate::{vector3_from_point, Vector3, EPSILON};
use shalrath::repr::{TexturePlane, TrianglePlane};

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
pub struct Plane3d {
    pub n: Vector3,
    pub d: f32,
}

impl Plane3d {
    pub fn normal(&self) -> &Vector3 {
        &self.n
    }

    pub fn distance(&self) -> f32 {
        self.d
    }

    // Returns true if the two planes are parallel
    pub fn is_parallel(&self, rhs: &Plane3d) -> bool {
        let plane_dot = self.normal().dot(rhs.normal());
        plane_dot <= -1.0 + EPSILON
    }

    // Returns true if the two planes are parallel and occupy the same position
    pub fn opposes(&self, rhs: &Plane3d) -> bool {
        if !self.is_parallel(rhs) {
            return false;
        }

        // Distances should be the same with different signs
        if self.distance().signum() == rhs.distance().signum() {
            return false;
        }

        // Distances should be within EPSILON of one another
        (self.distance().abs() - rhs.distance().abs()).abs() <= EPSILON
    }
}

impl From<&TrianglePlane> for Plane3d {
    fn from(t: &TrianglePlane) -> Self {
        let v0 = vector3_from_point(t.v0);
        let v1 = vector3_from_point(t.v1);
        let v2 = vector3_from_point(t.v2);

        let v0v1 = v1 - v0;
        let v0v2 = v2 - v0;

        let n = v0v2.cross(&v0v1).normalize();
        let d = n.dot(&v0);

        Plane3d { n, d }
    }
}

impl From<&TexturePlane> for Plane3d {
    fn from(p: &TexturePlane) -> Self {
        let n = nalgebra::vector![p.x, p.y, p.z];
        let d = p.d;
        Plane3d { n, d }
    }
}
