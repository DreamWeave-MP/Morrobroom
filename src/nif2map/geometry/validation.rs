use nalgebra::Matrix3;

use super::*;

pub(super) fn emitted_plane(line: &str) -> Result<(P3, f64), Error> {
    let mut rest = line;
    let mut points = Vec::new();
    for _ in 0..3 {
        let start = rest
            .find('(')
            .ok_or_else(|| Error::Reconstruction(format!("cannot parse emitted face: {line}")))?;
        let after_start = &rest[start + 1..];
        let end = after_start
            .find(')')
            .ok_or_else(|| Error::Reconstruction(format!("cannot parse emitted face: {line}")))?;
        let values: Vec<_> = after_start[..end]
            .split_whitespace()
            .filter_map(|value| value.parse::<f64>().ok())
            .collect();
        if values.len() != 3 {
            return Err(Error::Reconstruction(format!(
                "cannot parse emitted face: {line}"
            )));
        }
        points.push(P3 {
            x: values[0],
            y: values[1],
            z: values[2],
        });
        rest = &after_start[end + 1..];
    }
    let candidate = (points[2] - points[0]).cross(points[1] - points[0]);
    let area2 = candidate.norm();
    let max_edge = points[1]
        .distance(points[0])
        .max(points[2].distance(points[0]))
        .max(points[2].distance(points[1]));
    if area2 <= 0.0
        || max_edge <= 0.0
        || area2 / (max_edge * max_edge) < FACE_TRIPLET_QUALITY_EPSILON
    {
        return Err(Error::Reconstruction(
            "emitted face has an unstable plane definition".into(),
        ));
    }
    let normal = candidate / area2;
    Ok((normal, normal.dot(points[0])))
}

pub(super) fn solve_planes(first: (P3, f64), second: (P3, f64), third: (P3, f64)) -> Option<P3> {
    let matrix = Matrix3::from_rows(&[
        nalgebra::RowVector3::new(first.0.x, first.0.y, first.0.z),
        nalgebra::RowVector3::new(second.0.x, second.0.y, second.0.z),
        nalgebra::RowVector3::new(third.0.x, third.0.y, third.0.z),
    ]);
    matrix.try_inverse().map(|inverse| {
        let value = inverse * nalgebra::Vector3::new(first.1, second.1, third.1);
        P3 {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    })
}

pub(in crate::nif2map) fn validate_brush(brush: &Brush) -> Result<usize, Error> {
    let planes = if brush.planes.is_empty() {
        brush
            .faces
            .iter()
            .map(|face| emitted_plane(face))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        brush.planes.clone()
    };
    reject_duplicate_planes(&planes)?;
    let vertices = brush_vertices(&planes, brush)?;
    validate_brush_volume(&planes, &vertices)?;
    Ok(vertices.len())
}

pub(super) fn reject_duplicate_planes(planes: &[(P3, f64)]) -> Result<(), Error> {
    for first in 0..planes.len() {
        for second in (first + 1)..planes.len() {
            let dot = planes[first].0.dot(planes[second].0);
            if dot >= 1.0 - PLANE_DOT_EPSILON
                && (planes[first].1 - planes[second].1).abs() <= PLANE_DISTANCE_EPSILON
            {
                return Err(Error::Reconstruction(format!(
                    "brush contains duplicate coplanar faces ({first}, {second})"
                )));
            }
            if dot <= -1.0 + PLANE_DOT_EPSILON
                && (planes[first].1 + planes[second].1).abs() <= PLANE_DISTANCE_EPSILON
            {
                return Err(Error::Reconstruction(format!(
                    "brush has zero thickness between opposing coplanar faces ({first}, {second})"
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn brush_vertices(planes: &[(P3, f64)], brush: &Brush) -> Result<Vec<P3>, Error> {
    let mut vertices = Vec::new();
    for first in 0..planes.len() {
        for second in (first + 1)..planes.len() {
            for third in (second + 1)..planes.len() {
                let determinant = planes[first].0.dot(planes[second].0.cross(planes[third].0));
                if determinant.abs() <= 1e-10 {
                    continue;
                }
                let Some(point) = solve_planes(planes[first], planes[second], planes[third]) else {
                    continue;
                };
                if planes
                    .iter()
                    .all(|(normal, distance)| normal.dot(point) <= *distance + GEOMETRY_EPSILON)
                    && !vertices
                        .iter()
                        .any(|existing: &P3| existing.distance(point) <= WELD_EPSILON)
                {
                    vertices.push(point);
                }
            }
        }
    }
    if vertices.len() < 4 {
        return Err(Error::Reconstruction(format!(
            "{} brush from shapes {:?} is empty ({} reconstructed vertices)",
            brush.kind,
            brush.shapes,
            vertices.len()
        )));
    }
    Ok(vertices)
}

pub(super) fn validate_brush_volume(planes: &[(P3, f64)], vertices: &[P3]) -> Result<(), Error> {
    let center = average_points(vertices.to_vec());
    let scale = vertices
        .iter()
        .fold(P3::default(), |max, point| P3 {
            x: max.x.max((point.x - center.x).abs()),
            y: max.y.max((point.y - center.y).abs()),
            z: max.z.max((point.z - center.z).abs()),
        })
        .norm()
        .max(1.0);
    let mut rank3 = false;
    'rank: for first in 0..vertices.len() {
        for second in (first + 1)..vertices.len() {
            for third in (second + 1)..vertices.len() {
                if (vertices[second] - vertices[first])
                    .cross(vertices[third] - vertices[first])
                    .norm()
                    > scale * 1e-8
                {
                    rank3 = vertices.iter().any(|fourth| {
                        (vertices[second] - vertices[first])
                            .dot(
                                (vertices[third] - vertices[first])
                                    .cross(*fourth - vertices[first]),
                            )
                            .abs()
                            > scale.powi(3) * 1e-8
                    });
                    if rank3 {
                        break 'rank;
                    }
                }
            }
        }
    }
    if !rank3 {
        return Err(Error::Reconstruction(
            "brush half-space intersection has no 3D volume".into(),
        ));
    }
    let face_tolerance = (GEOMETRY_EPSILON * 4.0).max(scale * 1e-7);
    for (face_index, (normal, distance)) in planes.iter().enumerate() {
        let on_face: Vec<_> = vertices
            .iter()
            .copied()
            .filter(|point| (normal.dot(*point) - distance).abs() <= face_tolerance)
            .collect();
        if on_face.len() < 3 {
            return Err(Error::Reconstruction(format!(
                "face {face_index} is redundant or does not bound the brush ({} supporting vertices)",
                on_face.len()
            )));
        }
        let mut best_area = 0.0;
        let mut best_quality = 0.0;
        for first in 0..on_face.len() {
            for second in (first + 1)..on_face.len() {
                for third in (second + 1)..on_face.len() {
                    let candidate =
                        (on_face[third] - on_face[first]).cross(on_face[second] - on_face[first]);
                    let area = candidate.norm();
                    let edge = on_face[second]
                        .distance(on_face[first])
                        .max(on_face[third].distance(on_face[first]))
                        .max(on_face[third].distance(on_face[second]));
                    if edge > 0.0 {
                        let quality = area / edge.powi(2);
                        if area > best_area {
                            best_area = area;
                            best_quality = quality;
                        }
                    }
                }
            }
        }
        if best_area <= GEOMETRY_EPSILON.powi(2) || best_quality < FACE_TRIPLET_QUALITY_EPSILON {
            return Err(Error::Reconstruction(format!(
                "face {face_index} collapses to a line/point (quality {best_quality:.3e})"
            )));
        }
    }
    Ok(())
}

pub(in crate::nif2map) fn validate_reconstruction(
    result: &mut Reconstruction,
) -> Result<(), Error> {
    let mut validated = Vec::with_capacity(result.brushes.len());
    for brush in result.brushes.drain(..) {
        match validate_brush(&brush) {
            Ok(_) => validated.push(brush),
            Err(error) if brush.kind == "planar-prism-fallback" => {
                result.warnings.push(format!(
                    "dropped invalid fallback brush from shapes {:?}: {error}",
                    brush.shapes
                ));
            }
            Err(error) => return Err(error),
        }
    }
    result.brushes = validated;
    Ok(())
}
