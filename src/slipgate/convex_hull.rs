use crate::slipgate::{EPSILON, Plane3d, Vector3};

/// A convex hull described by a set of planes
#[derive(Debug, Clone)]
pub struct ConvexHull(Vec<Plane3d>);

impl<T: IntoIterator<Item = Plane3d>> From<T> for ConvexHull {
    fn from(planes: T) -> Self {
        let mut planes = planes.into_iter().collect::<Vec<_>>();

        // Map editors do not all emit the same face winding. Normalize a
        // consistently inside-out brush once, at the hull boundary, rather
        // than making every consumer guess which side of each plane is solid.
        // The average of the plane anchors is a stable interior probe for a
        // bounded convex brush.
        if !planes.is_empty() {
            let center = planes
                .iter()
                .map(|plane| plane.normal() * plane.distance())
                .sum::<Vector3>()
                / planes.len() as f32;
            let orientation = planes
                .iter()
                .map(|plane| plane.normal().dot(&center) - plane.distance())
                .sum::<f32>();

            if orientation > EPSILON * planes.len() as f32 {
                for plane in &mut planes {
                    plane.n = -plane.n;
                    plane.d = -plane.d;
                }
            }
        }

        ConvexHull(planes)
    }
}

impl ConvexHull {
    pub fn planes(&self) -> &[Plane3d] {
        &self.0
    }

    pub fn contains(&self, vertex: &Vector3) -> bool {
        for plane in &self.0 {
            // Keep the classification in f64; a many-sided prism can put a
            // vertex only a few f32 ulps from one of its side planes.
            let normal = plane.normal().map(f64::from);
            let point = vertex.map(f64::from);
            let proj = normal.dot(&point);
            let distance = f64::from(plane.distance());
            if proj > distance && (proj - distance).abs() > f64::from(EPSILON) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_planes(inverted: bool) -> Vec<Plane3d> {
        let mut planes = vec![
            Plane3d {
                n: nalgebra::vector![-1.0, 0.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![1.0, 0.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, -1.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 1.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 0.0, -1.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 0.0, 1.0],
                d: 1.0,
            },
        ];
        if inverted {
            for plane in &mut planes {
                plane.n = -plane.n;
                plane.d = -plane.d;
            }
        }
        planes
    }

    #[test]
    fn hull_contains_the_center_for_either_consistent_winding() {
        for inverted in [false, true] {
            let hull = ConvexHull::from(cube_planes(inverted));
            assert!(hull.contains(&nalgebra::vector![0.0, 0.0, 0.0]));
            assert!(!hull.contains(&nalgebra::vector![2.0, 0.0, 0.0]));
        }
    }
}
