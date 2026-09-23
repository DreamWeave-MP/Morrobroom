#![allow(
    clippy::wildcard_imports,
    reason = "The geometry implementation shares the parent importer model without duplicating its domain vocabulary."
)]

use super::*;

mod decomposition;
mod fallback;
mod pipeline;
mod sweep;
mod validation;

#[cfg(test)]
pub(super) use decomposition::flip_shared_diagonal;
pub(super) use decomposition::{
    decomposition_preserves_target, emit_with_local_repairs, greedy_convex_merge,
    validated_triangulation_decomposition,
};
pub(super) use fallback::planar_fallback;
pub(super) use pipeline::reconstruct;
pub(super) use sweep::{
    collect_caps, collect_segments, compatible_sweep, detect_sweep, exact_layer_invariance,
    profile_cycles, unproject, validate_profile,
};
pub(super) use validation::{validate_brush, validate_reconstruction};

pub(super) fn unit(v: P3) -> Result<P3, Error> {
    v.unit()
}

pub(super) fn canonical_direction(v: P3) -> Result<P3, Error> {
    let mut direction = unit(v)?;
    let components = [direction.x.abs(), direction.y.abs(), direction.z.abs()];
    let dominant = components
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i);
    let value = [direction.x, direction.y, direction.z][dominant];
    if value < 0.0 {
        direction = -direction;
    }
    Ok(direction)
}

pub(super) fn stable_basis(direction: P3) -> Result<(P3, P3), Error> {
    let axes = [P3::X, P3::Y, P3::Z];
    let reference = *axes
        .iter()
        .min_by(|a, b| {
            direction
                .dot(**a)
                .abs()
                .total_cmp(&direction.dot(**b).abs())
        })
        .unwrap_or(&P3::X);
    let u = unit(direction.cross(reference))?;
    let v = unit(direction.cross(u))?;
    Ok((u, v))
}

pub(super) fn fmt(value: f64) -> String {
    if (value - value.round()).abs() <= 1e-6 {
        return format!("{}", value.round() as i64);
    }
    format!("{value:.8}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

pub(super) const STATE_FLOAT_EPSILON: f32 = 1e-5;
pub(super) const CANONICAL_ALPHA: f32 = 1.0;
pub(super) const CANONICAL_ALPHA_TEST_THRESHOLD: u8 = 128;

pub(super) fn canonical_float(value: f32) -> String {
    let value = if value.abs() <= STATE_FLOAT_EPSILON {
        0.0
    } else {
        (value / STATE_FLOAT_EPSILON).round() * STATE_FLOAT_EPSILON
    };
    let mut text = format!("{value:.5}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" { "0".into() } else { text }
}

pub(super) fn fmt_point(point: P3) -> String {
    format!("{} {} {}", fmt(point.x), fmt(point.y), fmt(point.z))
}

pub(super) fn poly(coords: Vec<Coord<f64>>) -> Polygon<f64> {
    let mut ring = coords;
    if ring.first() != ring.last()
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    Polygon::new(LineString::from(ring), Vec::new())
}

pub(super) fn polygon_area(polygon: &Polygon<f64>) -> f64 {
    polygon.unsigned_area()
}

pub(super) fn ring_points(polygon: &Polygon<f64>) -> Vec<Coord<f64>> {
    polygon
        .exterior()
        .0
        .iter()
        .copied()
        .take_while(|_| true)
        .collect::<Vec<_>>()
}

pub(super) fn remove_collinear(mut points: Vec<Coord<f64>>) -> Vec<Coord<f64>> {
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points.dedup_by(|a, b| ((a.x - b.x).hypot(a.y - b.y)) <= WELD_EPSILON);
    let mut changed = true;
    while changed && points.len() > 3 {
        changed = false;
        let mut reduced = Vec::with_capacity(points.len());
        for index in 0..points.len() {
            let previous = points[(index + points.len() - 1) % points.len()];
            let here = points[index];
            let next = points[(index + 1) % points.len()];
            let incoming = p2(here.x - previous.x, here.y - previous.y);
            let outgoing = p2(next.x - here.x, next.y - here.y);
            let il = incoming.x.hypot(incoming.y);
            let ol = outgoing.x.hypot(outgoing.y);
            if il <= WELD_EPSILON || ol <= WELD_EPSILON {
                changed = true;
                continue;
            }
            let cross = incoming.x * outgoing.y - incoming.y * outgoing.x;
            let sine = cross.abs() / (il * ol);
            let same_direction = incoming.x * outgoing.x + incoming.y * outgoing.y > 0.0;
            let chord = p2(next.x - previous.x, next.y - previous.y);
            let chord_length = chord.x.hypot(chord.y);
            let distance = if chord_length > WELD_EPSILON {
                let relative = p2(here.x - previous.x, here.y - previous.y);
                (relative.x * chord.y - relative.y * chord.x).abs() / chord_length
            } else {
                f64::INFINITY
            };
            if same_direction && (sine <= COLLINEAR_SINE_EPSILON || distance <= WELD_EPSILON * 0.25)
            {
                changed = true;
            } else {
                reduced.push(here);
            }
        }
        points = reduced;
    }
    points
}

pub(super) fn polygon_is_convex(polygon: &Polygon<f64>) -> bool {
    if !polygon.interiors().is_empty() {
        return false;
    }
    let area = polygon_area(polygon);
    (area - polygon.convex_hull().unsigned_area()).abs() <= 1e-7 * area.max(1.0)
}

pub(super) fn representative(polygon: &Polygon<f64>) -> Point<f64> {
    polygon
        .centroid()
        .filter(|point| polygon.covers(point))
        .unwrap_or_else(|| Point::from(polygon.exterior().0[0]))
}

pub(super) fn relative_error(lhs: f64, rhs: f64) -> f64 {
    (lhs - rhs).abs() / lhs.abs().max(rhs.abs()).max(1.0)
}

pub(super) fn ring_length(ring: &LineString<f64>) -> f64 {
    ring.0
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum()
}

pub(super) fn boundary_hausdorff(lhs: &Polygon<f64>, rhs: &Polygon<f64>) -> f64 {
    lhs.exterior().hausdorff_distance(rhs.exterior())
}

pub(super) fn plane_from_points(points: [P3; 3]) -> Result<(P3, f64), Error> {
    let normal = unit((points[1] - points[0]).cross(points[2] - points[0]))?;
    Ok((normal, normal.dot(points[0])))
}

pub(super) fn fit_projection(
    points: [P3; 3],
    texcoords: [[f64; 2]; 3],
    texture_size: (u32, u32),
) -> Option<Projection> {
    let origin = points[0];
    let normal = unit((points[1] - origin).cross(points[2] - origin)).ok()?;
    let tangent_u = unit(points[1] - origin).ok()?;
    let tangent_v = unit(normal.cross(tangent_u)).ok()?;
    let matrix = Matrix3::new(
        points[0].distance(origin).mul_add(0.0, 0.0), // keep the row layout explicit below
        points[0].distance(origin).mul_add(0.0, 0.0),
        1.0,
        (points[1] - origin).dot(tangent_u),
        (points[1] - origin).dot(tangent_v),
        1.0,
        (points[2] - origin).dot(tangent_u),
        (points[2] - origin).dot(tangent_v),
        1.0,
    );
    // The first row is [0, 0, 1]; spelling it this way avoids a temporary
    // heap allocation while retaining the prototype's least-squares result
    // for a triangle (three equations, three unknowns).
    let inverse = matrix.try_inverse()?;
    let width = f64::from(texture_size.0);
    let height = f64::from(texture_size.1);
    let sfit = inverse
        * nalgebra::Vector3::new(
            texcoords[0][0] * width,
            texcoords[1][0] * width,
            texcoords[2][0] * width,
        );
    let tfit = inverse
        * nalgebra::Vector3::new(
            texcoords[0][1] * height,
            texcoords[1][1] * height,
            texcoords[2][1] * height,
        );
    let lift = |fit: nalgebra::Vector3<f64>| {
        let gradient = tangent_u * fit[0] + tangent_v * fit[1];
        let magnitude = gradient.norm();
        if magnitude <= 1e-12 {
            (normal, fit[2] - normal.dot(origin), 1.0)
        } else {
            (
                gradient / magnitude,
                fit[2] - gradient.dot(origin),
                1.0 / magnitude,
            )
        }
    };
    let (u, u_shift, u_scale) = lift(sfit);
    let (v, v_shift, v_scale) = lift(tfit);
    let mut max_error: f64 = 0.0;
    for (point, uv) in points.into_iter().zip(texcoords) {
        let actual_s = u.dot(point) / u_scale + u_shift;
        let actual_t = v.dot(point) / v_scale + v_shift;
        let expected_s = uv[0] * width;
        let expected_t = uv[1] * height;
        max_error = max_error
            .max((actual_s - expected_s).abs())
            .max((actual_t - expected_t).abs());
    }
    let projection = Projection {
        u,
        u_shift,
        u_scale,
        v,
        v_shift,
        v_scale,
        max_error,
    };
    projection_is_valid(&projection).then_some(projection)
}

pub(super) fn triangle_projection(mesh: &VisualMesh, index: usize) -> Option<Projection> {
    let uvs = mesh.uvs.as_ref()?;
    let texture_size = mesh.texture_size?.size;
    let triangle = mesh.triangles[index];
    fit_projection(
        [
            mesh.vertices[triangle[0]],
            mesh.vertices[triangle[1]],
            mesh.vertices[triangle[2]],
        ],
        [uvs[triangle[0]], uvs[triangle[1]], uvs[triangle[2]]],
        texture_size,
    )
}

pub(super) fn usable_triangle_projection(mesh: &VisualMesh, index: usize) -> Option<Projection> {
    triangle_projection(mesh, index).filter(projection_is_usable)
}

pub(super) fn projection_error(
    mesh: &VisualMesh,
    triangle_index: usize,
    projection: &Projection,
) -> f64 {
    let Some(uvs) = mesh.uvs.as_ref() else {
        return f64::INFINITY;
    };
    let Some(texture_size) = mesh.texture_size else {
        return f64::INFINITY;
    };
    let triangle = mesh.triangles[triangle_index];
    let width = f64::from(texture_size.size.0);
    let height = f64::from(texture_size.size.1);
    triangle
        .into_iter()
        .map(|vertex_index| {
            let point = mesh.vertices[vertex_index];
            let uv = uvs[vertex_index];
            let expected_s = uv[0] * width;
            let expected_t = uv[1] * height;
            let actual_s = projection.u.dot(point) / projection.u_scale + projection.u_shift;
            let actual_t = projection.v.dot(point) / projection.v_scale + projection.v_shift;
            if !actual_s.is_finite()
                || !actual_t.is_finite()
                || !expected_s.is_finite()
                || !expected_t.is_finite()
            {
                return f64::INFINITY;
            }
            let error = (actual_s - expected_s)
                .abs()
                .max((actual_t - expected_t).abs());
            if error.is_finite() {
                error
            } else {
                f64::INFINITY
            }
        })
        .fold(0.0, f64::max)
}

pub(super) fn can_merge_uv_triangle(
    mesh: &VisualMesh,
    triangle_index: usize,
    candidate_projection: Option<&Projection>,
    group_projection: Option<&Projection>,
) -> bool {
    if mesh.uvs.is_none() {
        return true;
    }
    let Some(group_projection) = group_projection else {
        return false;
    };
    let Some(candidate_projection) = candidate_projection else {
        return false;
    };
    projection_is_usable(group_projection)
        && projection_is_usable(candidate_projection)
        && projection_error(mesh, triangle_index, group_projection) <= UV_MERGE_TOLERANCE_TEXELS
}

fn boundary_probe(polygon: &Polygon<f64>) -> Option<Point<f64>> {
    if let Some(centroid) = polygon.centroid()
        && polygon.contains(&centroid)
    {
        return Some(centroid);
    }
    let centroid = polygon.centroid()?;
    for edge in polygon.exterior().0.windows(2) {
        let midpoint = Point::from(p2(
            f64::midpoint(edge[0].x, edge[1].x),
            f64::midpoint(edge[0].y, edge[1].y),
        ));
        let probe = Point::from(p2(
            f64::midpoint(midpoint.x(), centroid.x()),
            f64::midpoint(midpoint.y(), centroid.y()),
        ));
        if polygon.contains(&probe) {
            return Some(probe);
        }
    }
    None
}

fn normalize_ring(mut coordinates: Vec<Coord<f64>>, counter_clockwise: bool) -> LineString<f64> {
    if coordinates.first() == coordinates.last() {
        coordinates.pop();
    }
    let polygon = poly(coordinates.clone());
    if polygon.exterior().is_ccw() != counter_clockwise {
        coordinates.reverse();
    }
    coordinates.push(coordinates[0]);
    LineString::from(coordinates)
}

type QuantizedEdge = (Q, Q);
type BoundaryGraph = (
    HashMap<Q, Coord<f64>>,
    std::collections::BTreeSet<QuantizedEdge>,
    f64,
);

fn boundary_graph(
    mesh: &VisualMesh,
    triangle_indices: &[usize],
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
) -> Option<BoundaryGraph> {
    let mut incidences: HashMap<QuantizedEdge, Vec<(Q, Q)>> = HashMap::new();
    let mut points = HashMap::<Q, Coord<f64>>::new();
    let mut triangle_area = 0.0;
    for triangle_index in triangle_indices {
        let triangle = mesh.triangles[*triangle_index];
        let coordinates: Vec<_> = triangle
            .into_iter()
            .map(|vertex| {
                let point = mesh.vertices[vertex];
                p2(
                    (point - origin).dot(tangent_u),
                    (point - origin).dot(tangent_v),
                )
            })
            .collect();
        let polygon = poly(coordinates.clone());
        if polygon_area(&polygon) <= 1e-9 {
            continue;
        }
        let keys: Vec<_> = coordinates.iter().copied().map(qkey).collect();
        if keys[0] == keys[1] || keys[1] == keys[2] || keys[2] == keys[0] {
            return None;
        }
        triangle_area += polygon_area(&polygon);
        for (index, next) in [(0, 1), (1, 2), (2, 0)] {
            points.entry(keys[index]).or_insert(coordinates[index]);
            points.entry(keys[next]).or_insert(coordinates[next]);
            let edge = if keys[index] <= keys[next] {
                (keys[index], keys[next])
            } else {
                (keys[next], keys[index])
            };
            incidences
                .entry(edge)
                .or_default()
                .push((keys[index], keys[next]));
        }
    }
    if incidences.is_empty() {
        return Some((points, std::collections::BTreeSet::new(), triangle_area));
    }
    let mut boundary_edges = Vec::new();
    for (edge, incidence) in incidences {
        match incidence.as_slice() {
            [directed] => boundary_edges.push(*directed),
            [first, second] if first.0 == second.1 && first.1 == second.0 && edge.0 != edge.1 => {}
            _ => return None,
        }
    }
    let mut boundary = std::collections::BTreeSet::new();
    for (first, second) in boundary_edges {
        boundary.insert(if first <= second {
            (first, second)
        } else {
            (second, first)
        });
    }
    Some((points, boundary, triangle_area))
}

fn boundary_cycles(
    points: &HashMap<Q, Coord<f64>>,
    boundary: &std::collections::BTreeSet<QuantizedEdge>,
) -> Option<Vec<Polygon<f64>>> {
    let mut adjacency: HashMap<Q, Vec<Q>> = HashMap::new();
    for &(first, second) in boundary {
        adjacency.entry(first).or_default().push(second);
        adjacency.entry(second).or_default().push(first);
    }
    if adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return None;
    }
    for neighbors in adjacency.values_mut() {
        neighbors.sort_unstable();
    }
    let mut unvisited = boundary.clone();
    let mut cycles = Vec::new();
    while let Some(first_edge) = unvisited.iter().next().copied() {
        let (start, mut current) = first_edge;
        let mut previous = start;
        let mut keys = vec![start, current];
        unvisited.remove(&first_edge);
        while current != start {
            let neighbors = adjacency.get(&current)?;
            let following = if neighbors[0] == previous {
                neighbors[1]
            } else {
                neighbors[0]
            };
            let edge = if current <= following {
                (current, following)
            } else {
                (following, current)
            };
            if following == start {
                if edge != first_edge && !unvisited.remove(&edge) {
                    return None;
                }
                break;
            }
            if !unvisited.remove(&edge) {
                return None;
            }
            keys.push(following);
            previous = current;
            current = following;
            if keys.len() > adjacency.len() + 1 {
                return None;
            }
        }
        if keys.len() < 3 {
            return None;
        }
        let polygon = poly(
            keys.into_iter()
                .map(|key| points.get(&key).copied())
                .collect::<Option<Vec<_>>>()?,
        );
        if polygon_area(&polygon) <= 1e-9 {
            return None;
        }
        cycles.push(polygon);
    }
    unvisited.is_empty().then_some(cycles)
}

fn assemble_boundary_polygons(
    cycles: &[Polygon<f64>],
    triangle_area: f64,
) -> Option<Vec<Polygon<f64>>> {
    let mut parents = vec![None; cycles.len()];
    let probes: Vec<_> = cycles.iter().map(boundary_probe).collect::<Option<_>>()?;
    for child in 0..cycles.len() {
        let mut candidates: Vec<_> = (0..cycles.len())
            .filter(|&parent| {
                parent != child
                    && polygon_area(&cycles[parent]) > polygon_area(&cycles[child])
                    && cycles[parent].contains(&probes[child])
            })
            .collect();
        candidates.sort_by(|lhs, rhs| {
            polygon_area(&cycles[*lhs]).total_cmp(&polygon_area(&cycles[*rhs]))
        });
        parents[child] = candidates.first().copied();
    }
    for first in 0..cycles.len() {
        for second in (first + 1)..cycles.len() {
            let nested = parents[first] == Some(second) || parents[second] == Some(first);
            if !nested && cycles[first].intersects(&cycles[second]) {
                return None;
            }
        }
    }
    let depth = |index: usize| {
        let mut depth = 0;
        let mut current = parents[index];
        while let Some(parent) = current {
            depth += 1;
            current = parents[parent];
        }
        depth
    };
    let mut result = Vec::new();
    for outer in 0..cycles.len() {
        if depth(outer) % 2 != 0 {
            continue;
        }
        let holes = (0..cycles.len())
            .filter(|&candidate| parents[candidate] == Some(outer) && depth(candidate) % 2 == 1)
            .map(|candidate| normalize_ring(cycles[candidate].exterior().0.clone(), false))
            .collect();
        let exterior = normalize_ring(cycles[outer].exterior().0.clone(), true);
        result.push(Polygon::new(exterior, holes));
    }
    let result_area: f64 = result.iter().map(polygon_area).sum();
    ((result_area - triangle_area).abs() <= 1e-6_f64.max(triangle_area * 1e-7)).then_some(result)
}

pub(super) fn indexed_boundary_polygons(
    mesh: &VisualMesh,
    triangle_indices: &[usize],
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
) -> Option<Vec<Polygon<f64>>> {
    let (points, boundary, triangle_area) =
        boundary_graph(mesh, triangle_indices, origin, tangent_u, tangent_v)?;
    if boundary.is_empty() {
        return Some(Vec::new());
    }
    let cycles = boundary_cycles(&points, &boundary)?;
    assemble_boundary_polygons(&cycles, triangle_area)
}

fn indexed_triangle_components(
    mesh: &VisualMesh,
    triangle_indices: &[usize],
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
) -> Vec<Vec<usize>> {
    let valid: Vec<_> = triangle_indices
        .iter()
        .copied()
        .filter(|triangle_index| {
            let triangle = mesh.triangles[*triangle_index];
            let coordinates: Vec<_> = triangle
                .into_iter()
                .map(|vertex| {
                    let point = mesh.vertices[vertex];
                    p2(
                        (point - origin).dot(tangent_u),
                        (point - origin).dot(tangent_v),
                    )
                })
                .collect();
            polygon_area(&poly(coordinates)) > 1e-9
        })
        .collect();
    let mut edge_owner = HashMap::<QuantizedEdge, usize>::new();
    let mut adjacency = vec![Vec::new(); valid.len()];
    for (local_index, triangle_index) in valid.iter().enumerate() {
        let triangle = mesh.triangles[*triangle_index];
        let keys: Vec<_> = triangle
            .into_iter()
            .map(|vertex| {
                let point = mesh.vertices[vertex];
                qkey(p2(
                    (point - origin).dot(tangent_u),
                    (point - origin).dot(tangent_v),
                ))
            })
            .collect();
        for (first, second) in [(keys[0], keys[1]), (keys[1], keys[2]), (keys[2], keys[0])] {
            let edge = if first <= second {
                (first, second)
            } else {
                (second, first)
            };
            if let Some(previous) = edge_owner.insert(edge, local_index) {
                adjacency[local_index].push(previous);
                adjacency[previous].push(local_index);
            }
        }
    }
    let mut components = Vec::new();
    let mut visited = vec![false; valid.len()];
    for start in 0..valid.len() {
        if visited[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        visited[start] = true;
        while let Some(current) = stack.pop() {
            component.push(valid[current]);
            for &neighbor in &adjacency[current] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    components.sort_by_key(|component| component[0]);
    components
}

fn indexed_boundary_components(
    mesh: &VisualMesh,
    triangle_indices: &[usize],
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
) -> Option<Vec<Polygon<f64>>> {
    indexed_triangle_components(mesh, triangle_indices, origin, tangent_u, tangent_v)
        .into_iter()
        .map(|component| indexed_boundary_polygons(mesh, &component, origin, tangent_u, tangent_v))
        .collect::<Option<Vec<_>>>()
        .map(|components| components.into_iter().flatten().collect())
}

pub(super) fn decompose(target: &Polygon<f64>) -> Result<Vec<Polygon<f64>>, Error> {
    if polygon_is_convex(target) {
        return Ok(vec![target.clone()]);
    }
    let floor = validated_triangulation_decomposition(target)?;
    let merged = greedy_convex_merge(floor.clone());
    if decomposition_preserves_target(target, &merged) {
        Ok(merged)
    } else {
        Ok(floor)
    }
}

pub(super) fn tb_triplet(points: &[P3], desired_normal: P3) -> Result<(P3, P3, P3), Error> {
    let mut best: Option<(f64, f64, P3, P3, P3, P3)> = None;
    for a in 0..points.len() {
        for b in (a + 1)..points.len() {
            for c in (b + 1)..points.len() {
                let candidate = (points[c] - points[a]).cross(points[b] - points[a]);
                let area2 = candidate.norm();
                if area2 <= 0.0 {
                    continue;
                }
                let max_edge = points[b]
                    .distance(points[a])
                    .max(points[c].distance(points[a]))
                    .max(points[c].distance(points[b]));
                if max_edge <= WELD_EPSILON {
                    continue;
                }
                let quality = area2 / (max_edge * max_edge);
                let normal = candidate / area2;
                if normal.dot(desired_normal).abs() < 0.999 {
                    continue;
                }
                if best
                    .as_ref()
                    .is_none_or(|old| (area2, quality) > (old.0, old.1))
                {
                    best = Some((area2, quality, points[a], points[b], points[c], normal));
                }
            }
        }
    }
    let Some((_, quality, a, b, c, normal)) = best else {
        return Err(Error::Reconstruction(
            "could not construct non-degenerate map face".into(),
        ));
    };
    if quality < FACE_TRIPLET_QUALITY_EPSILON {
        return Err(Error::Reconstruction(format!(
            "map face is numerically degenerate (triplet quality {quality:.3e})"
        )));
    }
    if normal.dot(desired_normal) > 0.0 {
        Ok((a, b, c))
    } else {
        Ok((a, c, b))
    }
}

pub(super) fn generic_projection(normal: P3) -> Projection {
    let normal = P3 {
        x: normal.x.abs(),
        y: normal.y.abs(),
        z: normal.z.abs(),
    };
    let (u, v) = if normal.z >= normal.x && normal.z >= normal.y {
        (
            P3::X,
            P3 {
                x: 0.0,
                y: -1.0,
                z: 0.0,
            },
        )
    } else if normal.x >= normal.y {
        (
            P3::Y,
            P3 {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
        )
    } else {
        (
            P3::X,
            P3 {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
        )
    };
    Projection {
        u,
        u_shift: 0.0,
        u_scale: 1.0,
        v,
        v_shift: 0.0,
        v_scale: 1.0,
        max_error: 0.0,
    }
}

#[cfg(test)]
pub(super) fn emit_face(
    points: &[P3],
    normal: P3,
    material: &str,
    projection: Option<&Projection>,
) -> Result<String, Error> {
    emit_face_with_plane(points, normal, material, projection).map(|(face, _)| face)
}

fn emit_face_with_plane(
    points: &[P3],
    normal: P3,
    material: &str,
    projection: Option<&Projection>,
) -> Result<(String, (P3, f64)), Error> {
    let (a, b, c) = tb_triplet(points, normal)?;
    let face_normal = unit((c - a).cross(b - a))?;
    let mapping = projection
        .cloned()
        .unwrap_or_else(|| generic_projection(normal));
    let material = serialize_material_name(material);
    Ok((
        format!(
            "( {} ) ( {} ) ( {} ) {} [ {} {} {} {} ] [ {} {} {} {} ] 0 {} {}",
            fmt_point(a),
            fmt_point(b),
            fmt_point(c),
            material,
            fmt(mapping.u.x),
            fmt(mapping.u.y),
            fmt(mapping.u.z),
            fmt(mapping.u_shift),
            fmt(mapping.v.x),
            fmt(mapping.v.y),
            fmt(mapping.v.z),
            fmt(mapping.v_shift),
            fmt(mapping.u_scale),
            fmt(mapping.v_scale)
        ),
        (face_normal, face_normal.dot(a)),
    ))
}

fn serialize_material_name(material: &str) -> String {
    let should_quote = material.is_empty()
        || material
            .chars()
            .any(|character| matches!(character, '"' | '\\' | ' ' | '\t'));
    if !should_quote {
        return material.to_owned();
    }
    let mut escaped = String::with_capacity(material.len() + 2);
    escaped.push('"');
    for character in material.chars() {
        if matches!(character, '"' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped.push('"');
    escaped
}

pub(super) fn source_for_edge(
    sources: &[Segment],
    a: Coord<f64>,
    b: Coord<f64>,
) -> Option<&Segment> {
    let midpoint = p2(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
    sources.iter().find(|source| {
        point_segment_distance(source.a, source.b, a) <= 2e-3
            && point_segment_distance(source.a, source.b, b) <= 2e-3
            && point_segment_distance(source.a, source.b, midpoint) <= 2e-3
    })
}

pub(super) fn point_segment_distance(a: Coord<f64>, b: Coord<f64>, point: Coord<f64>) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length_squared = dx.mul_add(dx, dy * dy);
    if length_squared <= f64::EPSILON {
        return (a.x - point.x).hypot(a.y - point.y);
    }
    let projection = ((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared;
    let projection = projection.clamp(0.0, 1.0);
    let nearest = p2(a.x + projection * dx, a.y + projection * dy);
    (nearest.x - point.x).hypot(nearest.y - point.y)
}

pub(super) fn source_cap<'a>(caps: &'a [Cap], point: &Point<f64>) -> Option<&'a Cap> {
    caps.iter()
        .find(|cap| cap.polygon.buffer(1e-6).covers(point))
}

pub(super) struct ExtrusionSources<'a> {
    side: &'a [Segment],
    low: &'a [Cap],
    high: &'a [Cap],
}

pub(super) fn extrude_pieces(
    pieces: &[Polygon<f64>],
    sweep: &Sweep,
    sources: &ExtrusionSources<'_>,
    skip: &str,
    kind: &str,
    shapes: &[usize],
) -> Result<Vec<Brush>, Error> {
    let mut brushes = Vec::new();
    for (piece_index, piece) in pieces.iter().enumerate() {
        let mut coordinates = remove_collinear(ring_points(piece));
        if coordinates.len() < 3 {
            continue;
        }
        let mut clean = poly(coordinates.clone());
        if !clean.exterior().is_ccw() {
            coordinates.reverse();
            clean = poly(coordinates.clone());
        }
        let low: Vec<_> = coordinates
            .iter()
            .map(|point| unproject(*point, sweep.t_min, sweep))
            .collect();
        let high: Vec<_> = coordinates
            .iter()
            .map(|point| unproject(*point, sweep.t_max, sweep))
            .collect();
        let low_source = source_cap(sources.low, &representative(&clean));
        let high_source = source_cap(sources.high, &representative(&clean));
        let (low_face, low_plane) = emit_face_with_plane(
            &low,
            -sweep.direction,
            low_source.map_or(skip, |source| source.material.as_str()),
            low_source.and_then(|source| source.projection.as_ref()),
        )
        .map_err(|error| {
            Error::Reconstruction(format!("{kind} piece {piece_index} low face: {error}"))
        })?;
        let (high_face, high_plane) = emit_face_with_plane(
            &high,
            sweep.direction,
            high_source.map_or(skip, |source| source.material.as_str()),
            high_source.and_then(|source| source.projection.as_ref()),
        )
        .map_err(|error| {
            Error::Reconstruction(format!("{kind} piece {piece_index} high face: {error}"))
        })?;
        let mut faces = vec![low_face, high_face];
        let mut planes = vec![low_plane, high_plane];
        for index in 0..coordinates.len() {
            let next = (index + 1) % coordinates.len();
            let edge = p2(
                coordinates[next].x - coordinates[index].x,
                coordinates[next].y - coordinates[index].y,
            );
            let outward_2d = p2(edge.y, -edge.x);
            let length = outward_2d.x.hypot(outward_2d.y);
            let outward = sweep.u * (outward_2d.x / length) + sweep.v * (outward_2d.y / length);
            let quad = [low[index], low[next], high[next], high[index]];
            let source = source_for_edge(sources.side, coordinates[index], coordinates[next]);
            let (face, plane) = emit_face_with_plane(
                &quad,
                outward,
                source.map_or(skip, |source| source.material.as_str()),
                source.and_then(|source| source.projection.as_ref()),
            )
            .map_err(|error| {
                Error::Reconstruction(format!("{kind} piece {piece_index} side face: {error}"))
            })?;
            faces.push(face);
            planes.push(plane);
        }
        brushes.push(Brush {
            faces,
            planes,
            kind: kind.into(),
            shapes: shapes.to_vec(),
            ..Default::default()
        });
    }
    Ok(brushes)
}

pub(super) fn union_polygons(polygons: &[Polygon<f64>]) -> MultiPolygon<f64> {
    if let [polygon] = polygons {
        return MultiPolygon(vec![polygon.clone()]);
    }
    unary_union(polygons)
}

pub(super) fn exact_extrusion(
    meshes: &[&VisualMesh],
    sweep: &Sweep,
    skip: &str,
) -> Option<(Vec<Brush>, RecognizerReport)> {
    if !exact_layer_invariance(meshes, sweep) {
        return None;
    }
    let (low_caps, high_caps) = collect_caps(meshes, sweep);
    if low_caps.is_empty() || high_caps.is_empty() {
        return None;
    }
    let low_area = union_polygons(
        &low_caps
            .iter()
            .map(|cap| cap.polygon.clone())
            .collect::<Vec<_>>(),
    );
    let high_area = union_polygons(
        &high_caps
            .iter()
            .map(|cap| cap.polygon.clone())
            .collect::<Vec<_>>(),
    );
    let low_total: f64 = low_area.0.iter().map(polygon_area).sum();
    let high_total: f64 = high_area.0.iter().map(polygon_area).sum();
    if low_total <= 1e-6
        || high_total <= 1e-6
        || (low_total - high_total).abs() / low_total.max(high_total).max(1.0) > 5e-4
    {
        return None;
    }
    let position_tolerance = WELD_EPSILON * 4.0;
    let hausdorff = low_area
        .0
        .iter()
        .zip(&high_area.0)
        .map(|(low, high)| boundary_hausdorff(low, high))
        .fold(0.0, f64::max);
    if hausdorff > position_tolerance.max((low_total.max(high_total)).sqrt() * 2e-3) {
        return None;
    }
    let profile = low_area.0.first()?.clone();
    let mut pieces = Vec::new();
    let mut floor_pieces = Vec::new();
    for polygon in &low_area.0 {
        let floor = validated_triangulation_decomposition(polygon).ok()?;
        floor_pieces.extend(floor.clone());
        pieces.extend(decompose(polygon).unwrap_or(floor));
    }
    let shapes: Vec<_> = meshes.iter().map(|mesh| mesh.block).collect();
    let side_sources = collect_segments(meshes, sweep, None);
    let brushes = match extrude_pieces(
        &pieces,
        sweep,
        &ExtrusionSources {
            side: &side_sources,
            low: &low_caps,
            high: &high_caps,
        },
        skip,
        "exact-extrusion",
        &shapes,
    ) {
        Ok(brushes) if brushes.iter().all(|brush| validate_brush(brush).is_ok()) => brushes,
        _ => emit_with_local_repairs(floor_pieces, |piece| {
            let mut brushes = extrude_pieces(
                std::slice::from_ref(piece),
                sweep,
                &ExtrusionSources {
                    side: &side_sources,
                    low: &low_caps,
                    high: &high_caps,
                },
                skip,
                "exact-extrusion",
                &shapes,
            )?;
            if brushes.len() != 1 {
                return Err(Error::Reconstruction(format!(
                    "exact-extrusion floor piece emitted {} brushes instead of one",
                    brushes.len()
                )));
            }
            Ok(brushes.remove(0))
        })
        .ok()?,
    };
    let details = serde_json::json!({
        "sweep_direction": [sweep.direction.x, sweep.direction.y, sweep.direction.z],
        "sweep_layers": sweep.layers.len(),
        "sweep_span": [sweep.t_min, sweep.t_max],
        "endpoint_similarity": sweep.similarity,
        "profile_area": polygon_area(&profile),
        "profile_layers_verified": sweep.layers.len(),
        "brushes": brushes.len(),
    });
    Some((
        brushes,
        RecognizerReport {
            kind: "exact-extrusion".into(),
            shapes,
            details,
        },
    ))
}

pub(super) fn swept_shell(
    meshes: &[&VisualMesh],
    sweep: &Sweep,
    thickness: f64,
    skip: &str,
) -> Option<(Vec<Brush>, RecognizerReport)> {
    let (low_caps, high_caps) = collect_caps(meshes, sweep);
    let cap_area: f64 = low_caps
        .iter()
        .chain(&high_caps)
        .map(|cap| polygon_area(&cap.polygon))
        .sum();
    if cap_area > 1e-5 {
        return None;
    }
    let (segments, cycles, max_hausdorff, max_length, max_area) = validate_profile(meshes, sweep)?;
    let style = BufferStyle::new(thickness).line_join(LineJoin::Miter(10.0));
    let mut shell_polygons = Vec::new();
    for cavity in &cycles {
        let outer = cavity.buffer_with_style(style.clone());
        let ring = outer.difference(cavity);
        if ring.0.is_empty() {
            return None;
        }
        shell_polygons.extend(ring.0);
    }
    let mut pieces = Vec::new();
    let mut floor_pieces = Vec::new();
    for polygon in shell_polygons {
        let floor = validated_triangulation_decomposition(&polygon).ok()?;
        floor_pieces.extend(floor.clone());
        pieces.extend(decompose(&polygon).unwrap_or(floor));
    }
    let shapes: Vec<_> = meshes.iter().map(|mesh| mesh.block).collect();
    let brushes = match extrude_pieces(
        &pieces,
        sweep,
        &ExtrusionSources {
            side: &segments,
            low: &[],
            high: &[],
        },
        skip,
        "swept-shell",
        &shapes,
    ) {
        Ok(brushes) if brushes.iter().all(|brush| validate_brush(brush).is_ok()) => brushes,
        _ => emit_with_local_repairs(floor_pieces, |piece| {
            let mut brushes = extrude_pieces(
                std::slice::from_ref(piece),
                sweep,
                &ExtrusionSources {
                    side: &segments,
                    low: &[],
                    high: &[],
                },
                skip,
                "swept-shell",
                &shapes,
            )?;
            if brushes.len() != 1 {
                return Err(Error::Reconstruction(format!(
                    "swept-shell floor piece emitted {} brushes instead of one",
                    brushes.len()
                )));
            }
            Ok(brushes.remove(0))
        })
        .ok()?,
    };
    let details = serde_json::json!({
        "sweep_direction": [sweep.direction.x, sweep.direction.y, sweep.direction.z],
        "sweep_layers": sweep.layers.len(),
        "sweep_span": [sweep.t_min, sweep.t_max],
        "endpoint_similarity": sweep.similarity,
        "profile_components": cycles.len(),
        "profile_area": cycles.iter().map(polygon_area).sum::<f64>(),
        "profile_layers_verified": sweep.layers.len(),
        "profile_max_hausdorff": max_hausdorff,
        "profile_max_length_relative_error": max_length,
        "profile_max_area_relative_error": max_area,
        "shell_thickness": thickness,
        "brushes": brushes.len(),
    });
    Some((
        brushes,
        RecognizerReport {
            kind: "swept-shell".into(),
            shapes,
            details,
        },
    ))
}

fn connected_segment_positions(segment_cache: &[Vec<Segment>]) -> Vec<usize> {
    let endpoint_sets: Vec<HashSet<Q>> = segment_cache
        .iter()
        .map(|segments| {
            segments
                .iter()
                .flat_map(|segment| [qkey(segment.a), qkey(segment.b)])
                .collect()
        })
        .collect();
    let mut component_positions = Vec::new();
    let mut reachable = endpoint_sets[0].clone();
    loop {
        let mut changed = false;
        for position in 0..segment_cache.len().saturating_sub(1) {
            if component_positions.contains(&position) {
                continue;
            }
            if endpoint_sets[position + 1]
                .iter()
                .any(|key| reachable.contains(key))
            {
                component_positions.push(position);
                reachable.extend(endpoint_sets[position + 1].iter().copied());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    component_positions.sort_unstable();
    component_positions
}

fn cached_profile_score(
    segment_cache: &[Vec<Segment>],
    component_positions: &[usize],
    mask: usize,
) -> (f64, usize) {
    let mut edge_set = HashSet::new();
    let mut segments = Vec::new();
    for (component_index, &position) in component_positions.iter().enumerate() {
        if mask != usize::MAX && mask & (1 << component_index) == 0 {
            continue;
        }
        for segment in &segment_cache[position + 1] {
            let first = qkey(segment.a);
            let second = qkey(segment.b);
            let edge = if first <= second {
                (first, second)
            } else {
                (second, first)
            };
            if edge_set.insert(edge) {
                segments.push(segment);
            }
        }
    }
    for segment in &segment_cache[0] {
        let first = qkey(segment.a);
        let second = qkey(segment.b);
        let edge = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        if edge_set.insert(edge) {
            segments.push(segment);
        }
    }
    let Some(cycles) = profile_cycles(&segments) else {
        return (0.0, 0);
    };
    (cycles.iter().map(polygon_area).sum(), cycles.len())
}

pub(super) fn group_open_shell_shapes<'a>(
    seed: &'a VisualMesh,
    seed_sweep: &Sweep,
    remaining: &HashSet<usize>,
    meshes: &[&'a VisualMesh],
    cached_sweeps: &HashMap<usize, Option<Sweep>>,
) -> Vec<&'a VisualMesh> {
    let mut compatible = Vec::new();
    for mesh in meshes {
        let mesh = *mesh;
        if mesh.block == seed.block {
            continue;
        }
        if remaining.contains(&mesh.block)
            && cached_sweeps
                .get(&mesh.block)
                .and_then(|sweep| sweep.as_ref())
                .is_some_and(|sweep| compatible_sweep(seed_sweep, sweep))
        {
            compatible.push(mesh);
        }
    }
    let mut segment_cache = Vec::with_capacity(compatible.len() + 1);
    segment_cache.push(collect_segments(&[seed], seed_sweep, Some(0)));
    for mesh in &compatible {
        segment_cache.push(collect_segments(&[*mesh], seed_sweep, Some(0)));
    }
    let component_positions = connected_segment_positions(&segment_cache);
    if cached_profile_score(&segment_cache, &component_positions, 0).1 > 0 {
        return vec![seed];
    }
    let mut best = vec![seed];
    let mut best_score = 0.0;
    if component_positions.len() <= 4 {
        for mask in 1usize..(1usize << component_positions.len()) {
            let mut group = vec![seed];
            for (component_index, &position) in component_positions.iter().enumerate() {
                if mask & (1 << component_index) != 0 {
                    group.push(compatible[position]);
                }
            }
            let (score, count) = cached_profile_score(&segment_cache, &component_positions, mask);
            if count > 0 && score > best_score + 1e-6 {
                best_score = score;
                best = group;
            }
        }
    } else {
        let mut group = vec![seed];
        group.extend(
            component_positions
                .iter()
                .map(|&position| compatible[position]),
        );
        if cached_profile_score(&segment_cache, &component_positions, usize::MAX).1 > 0 {
            best = group;
        }
    }
    best
}

pub(super) fn average_points(points: Vec<P3>) -> P3 {
    let count = points.len().max(1) as f64;
    let sum = points
        .into_iter()
        .fold(P3::default(), |sum, point| sum + point);
    sum / count
}
