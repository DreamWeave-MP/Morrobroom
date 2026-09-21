use crate::slipgate::{ConvexHull, Plane3d, brush::BrushId, face::FaceId};

/// A double-precision point used by the reference CSG implementation.
pub type CsgVector = nalgebra::Vector3<f64>;

/// Numerical policy for polygon classification and cleanup.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryTolerance {
    pub plane: f64,
    pub vertex_merge: f64,
    pub collinear: f64,
    pub minimum_area: f64,
}

impl Default for GeometryTolerance {
    fn default() -> Self {
        Self {
            plane: 1.0e-9,
            vertex_merge: 1.0e-9,
            collinear: 1.0e-10,
            minimum_area: 1.0e-12,
        }
    }
}

/// The half-space `normal · point <= distance`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CsgPlane {
    pub normal: CsgVector,
    pub distance: f64,
}

impl From<&Plane3d> for CsgPlane {
    fn from(plane: &Plane3d) -> Self {
        Self {
            normal: plane.normal().map(f64::from),
            distance: f64::from(plane.distance()),
        }
    }
}

/// A convex planar polygon in the reference CSG coordinate space.
#[derive(Debug, Clone, PartialEq)]
pub struct CsgPolygon {
    pub vertices: Vec<CsgVector>,
}

impl CsgPolygon {
    pub fn new(vertices: Vec<CsgVector>) -> Option<Self> {
        let polygon = Self { vertices };
        polygon
            .is_valid(&GeometryTolerance::default())
            .then_some(polygon)
    }

    pub fn area(&self) -> f64 {
        if self.vertices.len() < 3 {
            return 0.0;
        }

        let normal = self
            .vertices
            .iter()
            .zip(self.vertices.iter().cycle().skip(1))
            .fold(CsgVector::zeros(), |sum, (lhs, rhs)| sum + lhs.cross(rhs));
        0.5 * normal.norm()
    }

    fn is_valid(&self, tolerance: &GeometryTolerance) -> bool {
        self.vertices
            .iter()
            .all(|vertex| vertex.iter().all(|value| value.is_finite()))
            && self.vertices.len() >= 3
            && self.area() >= tolerance.minimum_area
    }

    fn cleaned(mut self, tolerance: &GeometryTolerance) -> Option<Self> {
        if self.vertices.len() < 3 {
            return None;
        }

        self.vertices.dedup_by(|previous, current| {
            (&*previous - &*current).norm() <= tolerance.vertex_merge
        });
        if self.vertices.len() > 1
            && (self.vertices[0] - self.vertices[self.vertices.len() - 1]).norm()
                <= tolerance.vertex_merge
        {
            self.vertices.pop();
        }

        let mut changed = true;
        while changed && self.vertices.len() >= 3 {
            changed = false;
            let len = self.vertices.len();
            let mut kept = Vec::with_capacity(len);
            for index in 0..len {
                let previous = self.vertices[(index + len - 1) % len];
                let current = self.vertices[index];
                let next = self.vertices[(index + 1) % len];
                let lhs = current - previous;
                let rhs = next - current;
                if lhs.norm() <= tolerance.vertex_merge
                    || rhs.norm() <= tolerance.vertex_merge
                    || lhs.cross(&rhs).norm()
                        <= tolerance.collinear * lhs.norm().max(1.0) * rhs.norm().max(1.0)
                {
                    changed = true;
                } else {
                    kept.push(current);
                }
            }
            self.vertices = kept;
        }

        self.is_valid(tolerance).then_some(self)
    }
}

/// A source polygon together with the identity needed by later material and
/// diagnostic stages.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceFragment {
    pub source_face: FaceId,
    pub source_brush: BrushId,
    pub polygon: CsgPolygon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolygonSplit {
    pub outside: Option<CsgPolygon>,
    pub inside: Option<CsgPolygon>,
}

/// Split a polygon into the part outside and the part inside one brush plane.
pub fn split_polygon_by_plane(
    polygon: &CsgPolygon,
    plane: &CsgPlane,
    tolerance: GeometryTolerance,
) -> PolygonSplit {
    PolygonSplit {
        outside: clip_polygon(polygon, plane, tolerance, false),
        inside: clip_polygon(polygon, plane, tolerance, true),
    }
}

/// Subtract one convex brush from one source surface.
pub fn subtract_convex_brush(
    fragment: SurfaceFragment,
    planes: &[CsgPlane],
    tolerance: GeometryTolerance,
) -> Vec<SurfaceFragment> {
    let mut inside_candidate = fragment;
    let mut visible = Vec::new();

    for plane in planes {
        let split = split_polygon_by_plane(&inside_candidate.polygon, plane, tolerance);

        if let Some(outside) = split.outside {
            visible.push(SurfaceFragment {
                source_face: inside_candidate.source_face,
                source_brush: inside_candidate.source_brush,
                polygon: outside,
            });
        }

        match split.inside {
            Some(inside) => inside_candidate.polygon = inside,
            None => return visible,
        }
    }

    visible
}

/// Subtract an existing Slipgate convex hull without exposing its storage
/// representation to the CSG caller.
pub fn subtract_convex_hull(
    fragment: SurfaceFragment,
    hull: &ConvexHull,
    tolerance: GeometryTolerance,
) -> Vec<SurfaceFragment> {
    let planes = hull.planes().iter().map(CsgPlane::from).collect::<Vec<_>>();
    subtract_convex_brush(fragment, &planes, tolerance)
}

/// Subtract the union of several convex brushes from one source surface.
///
/// Applying each subtraction to every surviving fragment is deliberately
/// straightforward. This is the reference operation that later broad-phase
/// or spatial-index implementations must match.
pub fn subtract_convex_hulls<'a>(
    fragment: SurfaceFragment,
    hulls: impl IntoIterator<Item = &'a ConvexHull>,
    tolerance: GeometryTolerance,
) -> Vec<SurfaceFragment> {
    let mut visible = vec![fragment];

    for hull in hulls {
        visible = visible
            .into_iter()
            .flat_map(|fragment| subtract_convex_hull(fragment, hull, tolerance))
            .collect();
        if visible.is_empty() {
            break;
        }
    }

    visible
}

fn clip_polygon(
    polygon: &CsgPolygon,
    plane: &CsgPlane,
    tolerance: GeometryTolerance,
    keep_inside: bool,
) -> Option<CsgPolygon> {
    let mut clipped = Vec::new();
    let len = polygon.vertices.len();

    for index in 0..len {
        let previous = polygon.vertices[(index + len - 1) % len];
        let current = polygon.vertices[index];
        let previous_distance = signed_distance(previous, plane);
        let current_distance = signed_distance(current, plane);
        let previous_kept = is_kept(previous_distance, tolerance.plane, keep_inside);
        let current_kept = is_kept(current_distance, tolerance.plane, keep_inside);

        if current_kept != previous_kept {
            let denominator = previous_distance - current_distance;
            if denominator.abs() > f64::EPSILON {
                let ratio = previous_distance / denominator;
                clipped.push(previous + (current - previous) * ratio);
            }
        }
        if current_kept {
            clipped.push(current);
        }
    }

    CsgPolygon { vertices: clipped }.cleaned(&tolerance)
}

fn signed_distance(point: CsgVector, plane: &CsgPlane) -> f64 {
    plane.normal.dot(&point) - plane.distance
}

fn is_kept(distance: f64, tolerance: f64, keep_inside: bool) -> bool {
    if keep_inside {
        distance <= tolerance
    } else {
        distance > tolerance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: f64, max: f64) -> CsgPolygon {
        CsgPolygon::new(vec![
            nalgebra::vector![min, min, 0.0],
            nalgebra::vector![max, min, 0.0],
            nalgebra::vector![max, max, 0.0],
            nalgebra::vector![min, max, 0.0],
        ])
        .unwrap()
    }

    fn plane(normal: [f64; 3], distance: f64) -> CsgPlane {
        CsgPlane {
            normal: nalgebra::vector![normal[0], normal[1], normal[2]],
            distance,
        }
    }

    fn fragment(polygon: CsgPolygon) -> SurfaceFragment {
        SurfaceFragment {
            source_face: FaceId(7),
            source_brush: BrushId(3),
            polygon,
        }
    }

    #[test]
    fn split_polygon_returns_two_clean_halves() {
        let split = split_polygon_by_plane(
            &square(-1.0, 1.0),
            &plane([1.0, 0.0, 0.0], 0.0),
            GeometryTolerance::default(),
        );

        assert_eq!(split.inside.unwrap().area(), 2.0);
        assert_eq!(split.outside.unwrap().area(), 2.0);
    }

    #[test]
    fn subtraction_preserves_source_identity_and_area() {
        let planes = [
            plane([1.0, 0.0, 0.0], 0.25),
            plane([-1.0, 0.0, 0.0], 0.25),
            plane([0.0, 1.0, 0.0], 0.25),
            plane([0.0, -1.0, 0.0], 0.25),
        ];
        let fragments = subtract_convex_brush(
            fragment(square(-1.0, 1.0)),
            &planes,
            GeometryTolerance::default(),
        );

        assert_eq!(fragments.len(), 4);
        let area = fragments
            .iter()
            .map(|fragment| fragment.polygon.area())
            .sum::<f64>();
        assert!((area - 3.75).abs() < 1.0e-9);
        assert!(fragments.iter().all(|fragment| {
            fragment.source_face == FaceId(7) && fragment.source_brush == BrushId(3)
        }));
    }

    #[test]
    fn subtraction_discards_a_fully_covered_polygon() {
        let planes = [
            plane([1.0, 0.0, 0.0], 2.0),
            plane([-1.0, 0.0, 0.0], 2.0),
            plane([0.0, 1.0, 0.0], 2.0),
            plane([0.0, -1.0, 0.0], 2.0),
        ];

        assert!(
            subtract_convex_brush(
                fragment(square(-1.0, 1.0)),
                &planes,
                GeometryTolerance::default()
            )
            .is_empty()
        );
    }

    #[test]
    fn subtraction_accepts_existing_convex_hulls() {
        let hull = ConvexHull::from([
            Plane3d {
                n: nalgebra::vector![1.0, 0.0, 0.0],
                d: 0.25,
            },
            Plane3d {
                n: nalgebra::vector![-1.0, 0.0, 0.0],
                d: 0.25,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 1.0, 0.0],
                d: 0.25,
            },
            Plane3d {
                n: nalgebra::vector![0.0, -1.0, 0.0],
                d: 0.25,
            },
        ]);

        let fragments = subtract_convex_hull(
            fragment(square(-1.0, 1.0)),
            &hull,
            GeometryTolerance::default(),
        );
        assert_eq!(fragments.len(), 4);
    }

    #[test]
    fn subtraction_handles_a_union_of_convex_hulls() {
        let left = ConvexHull::from([
            Plane3d {
                n: nalgebra::vector![1.0, 0.0, 0.0],
                d: -0.25,
            },
            Plane3d {
                n: nalgebra::vector![-1.0, 0.0, 0.0],
                d: 0.75,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 1.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, -1.0, 0.0],
                d: 1.0,
            },
        ]);
        let right = ConvexHull::from([
            Plane3d {
                n: nalgebra::vector![1.0, 0.0, 0.0],
                d: 0.75,
            },
            Plane3d {
                n: nalgebra::vector![-1.0, 0.0, 0.0],
                d: -0.25,
            },
            Plane3d {
                n: nalgebra::vector![0.0, 1.0, 0.0],
                d: 1.0,
            },
            Plane3d {
                n: nalgebra::vector![0.0, -1.0, 0.0],
                d: 1.0,
            },
        ]);

        let fragments = subtract_convex_hulls(
            fragment(square(-1.0, 1.0)),
            &[left, right],
            GeometryTolerance::default(),
        );
        let area = fragments
            .iter()
            .map(|fragment| fragment.polygon.area())
            .sum::<f64>();

        assert!((area - 2.0).abs() < 1.0e-9);
        assert!(fragments.iter().all(|fragment| {
            fragment.source_face == FaceId(7) && fragment.source_brush == BrushId(3)
        }));
    }
}
