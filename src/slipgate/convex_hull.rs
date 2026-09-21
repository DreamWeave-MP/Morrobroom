use crate::slipgate::{EPSILON, Plane3d, Vector3};

/// A convex hull described by a set of planes
#[derive(Debug, Clone)]
pub struct ConvexHull(Vec<Plane3d>);

impl<T: IntoIterator<Item = Plane3d>> From<T> for ConvexHull {
    fn from(planes: T) -> Self {
        ConvexHull(planes.into_iter().collect())
    }
}

impl ConvexHull {
    pub fn contains(&self, vertex: &Vector3) -> bool {
        for plane in &self.0 {
            let proj = plane.normal().dot(vertex);
            if proj > plane.distance() && (proj - plane.distance()).abs() > EPSILON {
                return false;
            }
        }
        true
    }
}
