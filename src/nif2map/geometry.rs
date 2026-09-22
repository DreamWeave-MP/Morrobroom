#![allow(
    clippy::wildcard_imports,
    reason = "The geometry implementation shares the parent importer model without duplicating its domain vocabulary."
)]

use super::*;

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

pub(super) fn cluster_values(mut values: Vec<f64>, epsilon: f64) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    let Some(first) = values.first().copied() else {
        return Vec::new();
    };
    let mut clusters = vec![vec![first]];
    for value in values.into_iter().skip(1) {
        let current = clusters.last().unwrap();
        let center = current.iter().sum::<f64>() / current.len() as f64;
        if (value - center).abs() <= epsilon {
            clusters.last_mut().unwrap().push(value);
        } else {
            clusters.push(vec![value]);
        }
    }
    clusters
        .into_iter()
        .map(|cluster| cluster.iter().sum::<f64>() / cluster.len() as f64)
        .collect()
}

pub(super) fn nearest_layer(value: f64, layers: &[f64]) -> usize {
    layers
        .iter()
        .enumerate()
        .min_by(|a, b| (value - *a.1).abs().total_cmp(&(value - *b.1).abs()))
        .map_or(0, |(index, _)| index)
}

pub(super) fn unique_edges(mesh: &VisualMesh) -> Vec<(usize, usize)> {
    let mut edges = HashSet::new();
    for triangle in &mesh.triangles {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            edges.insert(if a < b { (a, b) } else { (b, a) });
        }
    }
    let mut result: Vec<_> = edges.into_iter().collect();
    result.sort_unstable();
    result
}

pub(super) fn candidate_directions(mesh: &VisualMesh) -> Vec<P3> {
    let mut buckets: Vec<(P3, usize)> = Vec::new();
    let mut add = |direction: P3, weight: usize| {
        if direction.norm() <= WELD_EPSILON {
            return;
        }
        let Ok(candidate) = canonical_direction(direction) else {
            return;
        };
        if let Some((_, count)) = buckets
            .iter_mut()
            .find(|(existing, _)| candidate.dot(*existing).abs() >= DIRECTION_DOT_EPSILON)
        {
            *count += weight;
        } else {
            buckets.push((candidate, weight));
        }
    };
    for (a, b) in unique_edges(mesh) {
        add(mesh.vertices[b] - mesh.vertices[a], 1);
    }
    for triangle in &mesh.triangles {
        if let Ok((normal, _)) = plane_from_points([
            mesh.vertices[triangle[0]],
            mesh.vertices[triangle[1]],
            mesh.vertices[triangle[2]],
        ]) {
            add(normal, 1);
        }
    }
    buckets.sort_by_key(|bucket| std::cmp::Reverse(bucket.1));
    let mut selected: Vec<P3> = buckets
        .into_iter()
        .take(16)
        .map(|(direction, _)| direction)
        .collect();
    for axis in [P3::X, P3::Y, P3::Z] {
        if !selected
            .iter()
            .any(|existing| axis.dot(*existing).abs() >= DIRECTION_DOT_EPSILON)
        {
            selected.push(axis);
        }
    }
    selected
}

pub(super) fn analyze_sweep(mesh: &VisualMesh, direction: P3) -> Option<Sweep> {
    let direction = canonical_direction(direction).ok()?;
    let (u, v) = stable_basis(direction).ok()?;
    let t_values: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|point| point.dot(direction))
        .collect();
    let extent = t_values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - t_values.iter().copied().fold(f64::INFINITY, f64::min);
    if extent <= WELD_EPSILON {
        return None;
    }
    let layers = cluster_values(t_values.clone(), LAYER_EPSILON.max(extent * 1e-6));
    if !(2..=64).contains(&layers.len()) {
        return None;
    }
    let assignments: Vec<_> = t_values
        .iter()
        .map(|value| nearest_layer(*value, &layers))
        .collect();
    let mut endpoint_sets = [HashSet::new(), HashSet::new()];
    for (vertex, assignment) in mesh.vertices.iter().zip(assignments) {
        if assignment == 0 || assignment == layers.len() - 1 {
            endpoint_sets[usize::from(assignment != 0)].insert(qkey(Coord {
                x: vertex.dot(u),
                y: vertex.dot(v),
            }));
        }
    }
    if endpoint_sets.iter().any(HashSet::is_empty) {
        return None;
    }
    let intersection = endpoint_sets[0].intersection(&endpoint_sets[1]).count();
    let union = endpoint_sets[0].union(&endpoint_sets[1]).count();
    let similarity = if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    };
    if similarity < 0.70 {
        return None;
    }
    let mut parallel_edges = 0;
    for (a, b) in unique_edges(mesh) {
        let delta = mesh.vertices[b] - mesh.vertices[a];
        if delta.norm() > WELD_EPSILON
            && delta.unit().ok()?.dot(direction).abs() >= DIRECTION_DOT_EPSILON
        {
            parallel_edges += 1;
        }
    }
    if parallel_edges < 2 && !(layers.len() == 2 && similarity >= 0.95) {
        return None;
    }
    let score = similarity * 1000.0
        + f64::from(parallel_edges) * 10.0
        + 500.0 / f64::from(u32::try_from(layers.len()).unwrap_or(u32::MAX))
        - f64::from(u32::try_from(layers.len()).unwrap_or(u32::MAX));
    Some(Sweep {
        direction,
        u,
        v,
        t_min: layers[0],
        t_max: *layers.last()?,
        layers,
        similarity,
        score,
    })
}

pub(super) fn detect_sweep(mesh: &VisualMesh) -> Option<Sweep> {
    candidate_directions(mesh)
        .into_iter()
        .filter_map(|direction| analyze_sweep(mesh, direction))
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

pub(super) fn compatible_sweep(lhs: &Sweep, rhs: &Sweep) -> bool {
    if lhs.direction.dot(rhs.direction).abs() < DIRECTION_DOT_EPSILON {
        return false;
    }
    let (rhs_min, rhs_max) = if lhs.direction.dot(rhs.direction) < 0.0 {
        (-rhs.t_max, -rhs.t_min)
    } else {
        (rhs.t_min, rhs.t_max)
    };
    (lhs.t_min - rhs_min).abs() <= 1e-2 && (lhs.t_max - rhs_max).abs() <= 1e-2
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

pub(super) fn project(point: P3, sweep: &Sweep) -> Coord<f64> {
    Coord {
        x: point.dot(sweep.u),
        y: point.dot(sweep.v),
    }
}
pub(super) fn unproject(point: Coord<f64>, t: f64, sweep: &Sweep) -> P3 {
    sweep.u * point.x + sweep.v * point.y + sweep.direction * t
}

pub(super) fn collect_caps(meshes: &[VisualMesh], sweep: &Sweep) -> (Vec<Cap>, Vec<Cap>) {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut low = Vec::new();
    let mut high = Vec::new();
    for mesh in meshes {
        for triangle_index in 0..mesh.triangles.len() {
            let triangle = mesh.triangles[triangle_index];
            let points = [
                mesh.vertices[triangle[0]],
                mesh.vertices[triangle[1]],
                mesh.vertices[triangle[2]],
            ];
            let values = points.map(|point| point.dot(sweep.direction));
            let destination = if values
                .into_iter()
                .all(|value| (value - sweep.t_min).abs() <= tolerance)
            {
                Some(&mut low)
            } else if values
                .into_iter()
                .all(|value| (value - sweep.t_max).abs() <= tolerance)
            {
                Some(&mut high)
            } else {
                None
            };
            let Some(destination) = destination else {
                continue;
            };
            let coordinates = points
                .into_iter()
                .map(|point| qkey(project(point, sweep)).as_coord())
                .collect::<Vec<_>>();
            let polygon = poly(coordinates);
            if polygon_area(&polygon) <= 1e-9 {
                continue;
            }
            destination.push(Cap {
                polygon,
                material: mesh.material.clone(),
                projection: usable_triangle_projection(mesh, triangle_index),
            });
        }
    }
    (low, high)
}

pub(super) fn collect_segments(
    meshes: &[VisualMesh],
    sweep: &Sweep,
    layer_index: Option<usize>,
) -> Vec<Segment> {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut collected: HashMap<(Q, Q), Segment> = HashMap::new();
    for mesh in meshes {
        for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
            let points = [
                mesh.vertices[triangle[0]],
                mesh.vertices[triangle[1]],
                mesh.vertices[triangle[2]],
            ];
            let values = points.map(|point| point.dot(sweep.direction));
            let projection = usable_triangle_projection(mesh, triangle_index);
            for (a_index, b_index) in [(0, 1), (1, 2), (2, 0)] {
                let layer_a = nearest_layer(values[a_index], &sweep.layers);
                let layer_b = nearest_layer(values[b_index], &sweep.layers);
                if layer_a != layer_b || layer_index.is_some_and(|index| index != layer_a) {
                    continue;
                }
                if (values[a_index] - sweep.layers[layer_a]).abs() > tolerance
                    || (values[b_index] - sweep.layers[layer_b]).abs() > tolerance
                {
                    continue;
                }
                let a = project(points[a_index], sweep);
                let b = project(points[b_index], sweep);
                if ((b.x - a.x).hypot(b.y - a.y)) <= WELD_EPSILON {
                    continue;
                }
                let a_key = qkey(a);
                let b_key = qkey(b);
                let key = if a_key <= b_key {
                    (a_key, b_key)
                } else {
                    (b_key, a_key)
                };
                collected.entry(key).or_insert_with(|| Segment {
                    a: a_key.as_coord(),
                    b: b_key.as_coord(),
                    material: mesh.material.clone(),
                    projection: projection.clone(),
                    shape: mesh.block,
                });
            }
        }
    }
    let mut result: Vec<_> = collected.into_values().collect();
    result.sort_by_key(|segment| (qkey(segment.a), qkey(segment.b), segment.shape));
    result
}

pub(super) fn profile_cycles(segments: &[Segment]) -> Option<Vec<Polygon<f64>>> {
    let mut adjacency: HashMap<Q, HashSet<Q>> = HashMap::new();
    let mut points: HashMap<Q, Coord<f64>> = HashMap::new();
    let mut edges = HashSet::new();
    for segment in segments {
        let a = qkey(segment.a);
        let b = qkey(segment.b);
        if a == b {
            continue;
        }
        let edge = if a <= b { (a, b) } else { (b, a) };
        if !edges.insert(edge) {
            continue;
        }
        points.entry(a).or_insert(segment.a);
        points.entry(b).or_insert(segment.b);
        adjacency.entry(a).or_default().insert(b);
        adjacency.entry(b).or_default().insert(a);
    }
    if edges.is_empty() || adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return None;
    }
    let mut unvisited = edges;
    let mut cycles = Vec::new();
    while let Some(first_edge) = unvisited.iter().next().copied() {
        let (start, mut current) = first_edge;
        let mut previous = start;
        let mut keys = vec![start, current];
        unvisited.remove(&first_edge);
        while current != start {
            let candidates: Vec<_> = adjacency[&current]
                .iter()
                .copied()
                .filter(|neighbor| *neighbor != previous)
                .collect();
            if candidates.len() != 1 {
                return None;
            }
            let following = candidates[0];
            let edge = if current <= following {
                (current, following)
            } else {
                (following, current)
            };
            if following == start {
                unvisited.remove(&edge);
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
        let polygon = poly(keys.into_iter().map(|key| points[&key]).collect());
        if polygon_area(&polygon) <= 1e-8 {
            return None;
        }
        cycles.push(polygon);
    }
    Some(cycles)
}

pub(super) fn profile_area_signature(cycles: &[Polygon<f64>]) -> Vec<f64> {
    let mut areas: Vec<_> = cycles.iter().map(polygon_area).collect();
    areas.sort_by(f64::total_cmp);
    areas
}

type ProfileValidation = (Vec<Segment>, Vec<Polygon<f64>>, f64, f64, f64);

pub(super) fn validate_profile(meshes: &[VisualMesh], sweep: &Sweep) -> Option<ProfileValidation> {
    let mut profiles = Vec::new();
    for layer in 0..sweep.layers.len() {
        let segments = collect_segments(meshes, sweep, Some(layer));
        let cycles = profile_cycles(&segments)?;
        profiles.push((segments, cycles));
    }
    let canonical_segments = profiles[0].0.clone();
    let canonical = &profiles[0].1;
    let canonical_length: f64 = canonical
        .iter()
        .map(|polygon| ring_length(polygon.exterior()))
        .sum();
    let canonical_areas = profile_area_signature(canonical);
    let (min_x, min_y, max_x, max_y) = canonical
        .iter()
        .flat_map(|polygon| polygon.exterior().0.iter())
        .fold(
            (
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ),
            |bounds, point| {
                (
                    bounds.0.min(point.x),
                    bounds.1.min(point.y),
                    bounds.2.max(point.x),
                    bounds.3.max(point.y),
                )
            },
        );
    let scale = (max_x - min_x).hypot(max_y - min_y).max(1.0);
    let position_tolerance = (WELD_EPSILON * 4.0).max(scale * 2e-6);
    let mut max_hausdorff: f64 = 0.0;
    let mut max_length: f64 = 0.0;
    let mut max_area: f64 = 0.0;
    for (_, cycles) in profiles.iter().skip(1) {
        if cycles.len() != canonical.len() {
            return None;
        }
        for (lhs, rhs) in canonical.iter().zip(cycles) {
            let hausdorff = boundary_hausdorff(lhs, rhs);
            if hausdorff > position_tolerance {
                return None;
            }
            max_hausdorff = max_hausdorff.max(hausdorff);
        }
        let length: f64 = cycles
            .iter()
            .map(|polygon| ring_length(polygon.exterior()))
            .sum();
        let length_error = relative_error(canonical_length, length);
        if length_error > 2e-5 {
            return None;
        }
        max_length = max_length.max(length_error);
        for (lhs, rhs) in canonical_areas.iter().zip(profile_area_signature(cycles)) {
            let error = relative_error(*lhs, rhs);
            if error > 2e-5 {
                return None;
            }
            max_area = max_area.max(error);
        }
    }
    for (index, first) in canonical.iter().enumerate() {
        for second in canonical.iter().skip(index + 1) {
            if first.intersects(second)
                || first.contains(&representative(second))
                || second.contains(&representative(first))
            {
                return None;
            }
        }
    }
    Some((
        canonical_segments,
        canonical.clone(),
        max_hausdorff,
        max_length,
        max_area,
    ))
}

pub(super) fn projected_layer_sets(meshes: &[VisualMesh], sweep: &Sweep) -> Vec<HashSet<Q>> {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut result = vec![HashSet::new(); sweep.layers.len()];
    for mesh in meshes {
        for vertex in &mesh.vertices {
            let layer = nearest_layer(vertex.dot(sweep.direction), &sweep.layers);
            if (vertex.dot(sweep.direction) - sweep.layers[layer]).abs() <= tolerance {
                result[layer].insert(qkey(project(*vertex, sweep)));
            }
        }
    }
    result
}

pub(super) fn exact_layer_invariance(meshes: &[VisualMesh], sweep: &Sweep) -> bool {
    let layers = projected_layer_sets(meshes, sweep);
    let Some(canonical) = layers.first() else {
        return false;
    };
    if canonical.is_empty() {
        return false;
    }
    layers.iter().skip(1).all(|layer| {
        if layer.is_empty() {
            return false;
        }
        let intersection = canonical.intersection(layer).count();
        let union = canonical.union(layer).count();
        union > 0 && intersection as f64 / union as f64 >= 0.999
    })
}

pub(super) fn triangulation_decomposition(target: &Polygon<f64>) -> Vec<Polygon<f64>> {
    target
        .earcut_triangles()
        .into_iter()
        .filter_map(|triangle| {
            let polygon = poly(vec![triangle.v1(), triangle.v2(), triangle.v3()]);
            (polygon_area(&polygon) > 1e-9 && target.covers(&representative(&polygon)))
                .then_some(polygon)
        })
        .collect()
}

pub(super) fn decomposition_score(target: &Polygon<f64>, pieces: &[Polygon<f64>]) -> f64 {
    if pieces.is_empty() {
        return f64::INFINITY;
    }
    let characteristic = polygon_area(target).max(1e-9).sqrt().max(1.0);
    let perimeter: f64 = pieces
        .iter()
        .map(|piece| ring_length(piece.exterior()))
        .sum();
    let internal = ((perimeter - ring_length(target.exterior())) * 0.5).max(0.0);
    let sliver_penalty: f64 = pieces
        .iter()
        .map(|piece| {
            let (min_x, min_y, max_x, max_y) = piece.exterior().0.iter().fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |bounds, point| {
                    (
                        bounds.0.min(point.x),
                        bounds.1.min(point.y),
                        bounds.2.max(point.x),
                        bounds.3.max(point.y),
                    )
                },
            );
            let width = (max_x - min_x).max(1e-9);
            let height = (max_y - min_y).max(1e-9);
            let aspect = (width / height).max(height / width);
            (aspect - 12.0).max(0.0)
        })
        .sum();
    pieces.len() as f64 * 12.0 + internal / characteristic * 3.0 + sliver_penalty
}

pub(super) fn rounded_level(value: f64) -> f64 {
    (value * 1e8).round() / 1e8
}

pub(super) fn slice_decomposition(target: &Polygon<f64>, axis: usize) -> Vec<Polygon<f64>> {
    let mut levels: Vec<_> = target
        .exterior()
        .0
        .iter()
        .map(|point| rounded_level(if axis == 0 { point.x } else { point.y }))
        .collect();
    for ring in target.interiors() {
        levels.extend(
            ring.0
                .iter()
                .map(|point| rounded_level(if axis == 0 { point.x } else { point.y })),
        );
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|lhs, rhs| (*lhs - *rhs).abs() <= 1e-12);
    if levels.len() < 2 {
        return Vec::new();
    }
    let (min_x, min_y, max_x, max_y) = target.exterior().0.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |bounds, point| {
            (
                bounds.0.min(point.x),
                bounds.1.min(point.y),
                bounds.2.max(point.x),
                bounds.3.max(point.y),
            )
        },
    );
    let margin = (max_x - min_x).max(max_y - min_y).max(1.0) * 4.0;
    let mut pieces = Vec::new();
    for pair in levels.windows(2) {
        let low = pair[0];
        let high = pair[1];
        if high - low <= 1e-8 {
            continue;
        }
        let slab = if axis == 0 {
            poly(vec![
                p2(low, min_y - margin),
                p2(high, min_y - margin),
                p2(high, max_y + margin),
                p2(low, max_y + margin),
            ])
        } else {
            poly(vec![
                p2(min_x - margin, low),
                p2(max_x + margin, low),
                p2(max_x + margin, high),
                p2(min_x - margin, high),
            ])
        };
        let intersection = target.intersection(&slab);
        for polygon in intersection.0 {
            if polygon_area(&polygon) <= 1e-8 {
                continue;
            }
            let coordinates = remove_collinear(ring_points(&polygon));
            if coordinates.len() < 3 {
                continue;
            }
            let clean = Polygon::new(
                LineString::from({
                    let mut ring = coordinates;
                    let first = ring[0];
                    ring.push(first);
                    ring
                }),
                polygon.interiors().to_vec(),
            );
            if polygon_is_convex(&clean) {
                pieces.push(clean);
            } else {
                pieces.extend(triangulation_decomposition(&clean));
            }
        }
    }
    greedy_convex_merge(pieces)
}

pub(super) fn greedy_convex_merge(polygons: Vec<Polygon<f64>>) -> Vec<Polygon<f64>> {
    let mut pieces = polygons;
    loop {
        let mut best: Option<(f64, usize, usize, Polygon<f64>)> = None;
        for i in 0..pieces.len() {
            for j in (i + 1)..pieces.len() {
                // A shared boundary is a cheap broad-phase before invoking the
                // considerably more expensive overlay operation.
                let shared = pieces[i].exterior().0.iter().any(|a| {
                    pieces[j]
                        .exterior()
                        .0
                        .iter()
                        .any(|b| (a.x - b.x).hypot(a.y - b.y) <= WELD_EPSILON)
                });
                if !shared {
                    continue;
                }
                let merged = pieces[i].union(&pieces[j]);
                if merged.0.len() != 1 {
                    continue;
                }
                let merged = merged.0.into_iter().next().unwrap();
                if !polygon_is_convex(&merged) {
                    continue;
                }
                let (min_x, min_y, max_x, max_y) = merged.exterior().0.iter().fold(
                    (
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    ),
                    |bounds, point| {
                        (
                            bounds.0.min(point.x),
                            bounds.1.min(point.y),
                            bounds.2.max(point.x),
                            bounds.3.max(point.y),
                        )
                    },
                );
                let width = (max_x - min_x).max(1e-9);
                let height = (max_y - min_y).max(1e-9);
                let aspect = (width / height).max(height / width);
                let score = 1.0 - (aspect - 8.0).max(0.0);
                if best.as_ref().is_none_or(|candidate| score > candidate.0) {
                    best = Some((score, i, j, merged));
                }
            }
        }
        let Some((_, i, j, merged)) = best else {
            break;
        };
        pieces = pieces
            .into_iter()
            .enumerate()
            .filter_map(|(index, piece)| (index != i && index != j).then_some(piece))
            .collect();
        pieces.push(merged);
    }
    pieces
}

pub(super) fn decompose(target: &Polygon<f64>) -> Result<Vec<Polygon<f64>>, Error> {
    let mut candidates = vec![greedy_convex_merge(triangulation_decomposition(target))];
    candidates.extend([
        slice_decomposition(target, 0),
        slice_decomposition(target, 1),
    ]);
    let area = polygon_area(target);
    candidates.retain(|pieces| {
        !pieces.is_empty()
            && (pieces.iter().map(polygon_area).sum::<f64>() - area).abs()
                <= 1e-6_f64.max(area * 1e-8)
    });
    candidates
        .into_iter()
        .min_by(|lhs, rhs| {
            decomposition_score(target, lhs).total_cmp(&decomposition_score(target, rhs))
        })
        .ok_or_else(|| Error::Reconstruction("could not convex-decompose 2D target".into()))
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

pub(super) fn emit_face(
    points: &[P3],
    normal: P3,
    material: &str,
    projection: Option<&Projection>,
) -> Result<String, Error> {
    let (a, b, c) = tb_triplet(points, normal)?;
    let mapping = projection
        .cloned()
        .unwrap_or_else(|| generic_projection(normal));
    Ok(format!(
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
    ))
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
    for piece in pieces {
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
        let mut faces = vec![
            emit_face(
                &low,
                -sweep.direction,
                low_source.map_or(skip, |source| source.material.as_str()),
                low_source.and_then(|source| source.projection.as_ref()),
            )?,
            emit_face(
                &high,
                sweep.direction,
                high_source.map_or(skip, |source| source.material.as_str()),
                high_source.and_then(|source| source.projection.as_ref()),
            )?,
        ];
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
            faces.push(emit_face(
                &quad,
                outward,
                source.map_or(skip, |source| source.material.as_str()),
                source.and_then(|source| source.projection.as_ref()),
            )?);
        }
        brushes.push(Brush {
            faces,
            kind: kind.into(),
            shapes: shapes.to_vec(),
            ..Default::default()
        });
    }
    Ok(brushes)
}

pub(super) fn union_polygons(polygons: &[Polygon<f64>]) -> MultiPolygon<f64> {
    unary_union(polygons)
}

pub(super) fn exact_extrusion(
    meshes: &[VisualMesh],
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
    for polygon in &low_area.0 {
        pieces.extend(decompose(polygon).ok()?);
    }
    let shapes: Vec<_> = meshes.iter().map(|mesh| mesh.block).collect();
    let side_sources = collect_segments(meshes, sweep, None);
    let brushes = extrude_pieces(
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
    )
    .ok()?;
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
    meshes: &[VisualMesh],
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
    for polygon in shell_polygons {
        pieces.extend(decompose(&polygon).ok()?);
    }
    let shapes: Vec<_> = meshes.iter().map(|mesh| mesh.block).collect();
    let brushes = extrude_pieces(
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
    )
    .ok()?;
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

pub(super) fn group_open_shell_shapes(
    seed: &VisualMesh,
    seed_sweep: &Sweep,
    remaining: &[VisualMesh],
    cached_sweeps: &HashMap<usize, Option<Sweep>>,
) -> Vec<VisualMesh> {
    let mut compatible = Vec::new();
    for mesh in remaining {
        if mesh.block == seed.block {
            continue;
        }
        if cached_sweeps
            .get(&mesh.block)
            .and_then(|sweep| sweep.as_ref())
            .is_some_and(|sweep| compatible_sweep(seed_sweep, sweep))
        {
            compatible.push(mesh.clone());
        }
    }
    let closed_score = |group: &[VisualMesh]| -> (f64, usize) {
        let segments = collect_segments(group, seed_sweep, Some(0));
        let Some(cycles) = profile_cycles(&segments) else {
            return (0.0, 0);
        };
        (cycles.iter().map(polygon_area).sum(), cycles.len())
    };
    if closed_score(std::slice::from_ref(seed)).1 > 0 {
        return vec![seed.clone()];
    }
    let mut best = vec![seed.clone()];
    let mut best_score = 0.0;
    if compatible.len() <= 8 {
        for mask in 1usize..(1usize << compatible.len()) {
            let mut group = vec![seed.clone()];
            for (index, mesh) in compatible.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    group.push(mesh.clone());
                }
            }
            let (score, count) = closed_score(&group);
            if count > 0 && score > best_score + 1e-6 {
                best_score = score;
                best = group;
            }
        }
    } else {
        let mut group = vec![seed.clone()];
        group.extend(compatible);
        if closed_score(&group).1 > 0 {
            best = group;
        }
    }
    best
}

pub(super) fn canonical_plane(normal: P3, distance: f64) -> (P3, f64) {
    let values = [normal.x, normal.y, normal.z];
    for value in values {
        if value.abs() > 1e-10 {
            return if value < 0.0 {
                (-normal, -distance)
            } else {
                (normal, distance)
            };
        }
    }
    (normal, distance)
}

pub(super) fn planar_fallback(
    mesh: &VisualMesh,
    thickness: f64,
    skip: &str,
) -> Result<(Vec<Brush>, RecognizerReport), Error> {
    // Coplanar triangles may belong to different UV charts. Merge only when
    // one affine projection reproduces the candidate triangle within tolerance.
    let mut groups: Vec<(P3, f64, Vec<usize>, Option<Projection>)> = Vec::new();
    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        let points = [
            mesh.vertices[triangle[0]],
            mesh.vertices[triangle[1]],
            mesh.vertices[triangle[2]],
        ];
        let Ok((normal, distance)) = plane_from_points(points) else {
            continue;
        };
        let (normal, distance) = canonical_plane(normal, distance);
        let projection = usable_triangle_projection(mesh, triangle_index);
        if let Some((_, _, indices, _)) = groups.iter_mut().find(
            |(candidate, candidate_distance, _indices, group_projection)| {
                candidate.dot(normal).abs() >= 0.99999
                    && (*candidate_distance - distance).abs() <= 1e-3
                    && (mesh.uvs.is_none()
                        || can_merge_uv_triangle(
                            mesh,
                            triangle_index,
                            projection.as_ref(),
                            group_projection.as_ref(),
                        ))
            },
        ) {
            indices.push(triangle_index);
        } else {
            groups.push((normal, distance, vec![triangle_index], projection));
        }
    }
    let mut brushes = Vec::new();
    for (_, _, triangle_indices, projection) in &groups {
        brushes.extend(fallback_group(
            mesh,
            triangle_indices,
            projection.as_ref(),
            thickness,
            skip,
        )?);
    }
    let details = serde_json::json!({ "plane_groups": groups.len(), "thickness": thickness, "brushes": brushes.len() });
    Ok((
        brushes,
        RecognizerReport {
            kind: "planar-prism-fallback".into(),
            shapes: vec![mesh.block],
            details,
        },
    ))
}

pub(super) fn fallback_group(
    mesh: &VisualMesh,
    triangle_indices: &[usize],
    projection: Option<&Projection>,
    thickness: f64,
    skip: &str,
) -> Result<Vec<Brush>, Error> {
    let first = mesh.triangles[triangle_indices[0]];
    let first_points = [
        mesh.vertices[first[0]],
        mesh.vertices[first[1]],
        mesh.vertices[first[2]],
    ];
    let (authored_normal, _) = plane_from_points(first_points)
        .map_err(|error| Error::Reconstruction(error.to_string()))?;
    let origin = first_points[0];
    let tangent_u = unit(first_points[1] - origin)?;
    let tangent_v = unit(authored_normal.cross(tangent_u))?;
    let mut polygons = Vec::new();
    for triangle_index in triangle_indices {
        let triangle = mesh.triangles[*triangle_index];
        let points = [
            mesh.vertices[triangle[0]],
            mesh.vertices[triangle[1]],
            mesh.vertices[triangle[2]],
        ];
        let polygon = poly(
            points
                .into_iter()
                .map(|point| {
                    p2(
                        (point - origin).dot(tangent_u),
                        (point - origin).dot(tangent_v),
                    )
                })
                .collect(),
        );
        if polygon_area(&polygon) > 1e-9 {
            polygons.push(polygon);
        }
    }
    if polygons.is_empty() {
        return Ok(Vec::new());
    }
    let context = FallbackContext {
        mesh,
        projection,
        origin,
        tangent_u,
        tangent_v,
        authored_normal,
        thickness,
        skip,
    };
    let mut brushes = Vec::new();
    for region_polygon in union_polygons(&polygons).0 {
        for piece in decompose(&region_polygon)? {
            if let Some(brush) = fallback_piece(&context, &piece) {
                brushes.push(brush?);
            }
        }
    }
    Ok(brushes)
}

pub(super) struct FallbackContext<'a> {
    mesh: &'a VisualMesh,
    projection: Option<&'a Projection>,
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
    authored_normal: P3,
    thickness: f64,
    skip: &'a str,
}

pub(super) fn fallback_piece(
    context: &FallbackContext<'_>,
    piece: &Polygon<f64>,
) -> Option<Result<Brush, Error>> {
    let mut coordinates = remove_collinear(ring_points(piece));
    if coordinates.len() < 3 {
        return None;
    }
    if !poly(coordinates.clone()).exterior().is_ccw() {
        coordinates.reverse();
    }
    let front: Vec<_> = coordinates
        .iter()
        .map(|point| context.origin + context.tangent_u * point.x + context.tangent_v * point.y)
        .collect();
    let back: Vec<_> = front
        .iter()
        .map(|point| *point - context.authored_normal * context.thickness)
        .collect();
    let source_projection = context.projection;
    let mut faces = vec![
        emit_face(
            &front,
            context.authored_normal,
            &context.mesh.material,
            source_projection,
        ),
        emit_face(&back, -context.authored_normal, context.skip, None),
    ];
    let center = average_points(front.iter().chain(&back).copied().collect());
    for index in 0..coordinates.len() {
        let next = (index + 1) % coordinates.len();
        let quad = [front[index], back[index], back[next], front[next]];
        let side_normal = unit((quad[1] - quad[0]).cross(quad[2] - quad[0]));
        let side_normal = side_normal.map(|normal| {
            if normal.dot(average_points(quad.to_vec()) - center) < 0.0 {
                -normal
            } else {
                normal
            }
        });
        faces.push(side_normal.and_then(|normal| emit_face(&quad, normal, context.skip, None)));
    }
    Some(
        faces
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map(|faces| Brush {
                faces,
                kind: "planar-prism-fallback".into(),
                shapes: vec![context.mesh.block],
                ..Default::default()
            }),
    )
}

pub(super) fn average_points(points: Vec<P3>) -> P3 {
    let count = points.len().max(1) as f64;
    let sum = points
        .into_iter()
        .fold(P3::default(), |sum, point| sum + point);
    sum / count
}

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

pub(super) fn validate_brush(brush: &Brush) -> Result<usize, Error> {
    let planes: Vec<_> = brush
        .faces
        .iter()
        .map(|face| emitted_plane(face))
        .collect::<Result<_, _>>()?;
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

pub(super) fn reconstruct(
    meshes: &[VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut partitions: BTreeMap<(ScopeId, NifState), Vec<VisualMesh>> = BTreeMap::new();
    for mesh in meshes {
        partitions
            .entry((mesh.scope, mesh.nif_state.clone()))
            .or_default()
            .push(mesh.clone());
    }
    let mut result = Reconstruction::default();
    for ((scope, nif_state), partition) in partitions {
        let mut partial = reconstruct_partition(&partition, options)?;
        for brush in &mut partial.brushes {
            brush.scope = scope;
            brush.nif_state = nif_state.clone();
        }
        result.brushes.extend(partial.brushes);
        result.used_shapes.extend(partial.used_shapes);
        result.recognizers.extend(partial.recognizers);
        result.warnings.extend(partial.warnings);
        result.uv_max_error = result.uv_max_error.max(partial.uv_max_error);
    }
    if result.brushes.len() > options.max_brushes {
        return Err(Error::Reconstruction(format!(
            "reconstruction produced {} brushes, exceeding --max-brushes {}",
            result.brushes.len(),
            options.max_brushes
        )));
    }
    Ok(result)
}

pub(super) fn reconstruct_partition(
    meshes: &[VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut result = Reconstruction::default();
    let mut remaining: HashMap<usize, VisualMesh> = meshes
        .iter()
        .cloned()
        .map(|mesh| (mesh.block, mesh))
        .collect();
    let sweeps: HashMap<usize, Option<Sweep>> = meshes
        .par_iter()
        .map(|mesh| (mesh.block, detect_sweep(mesh)))
        .collect();
    let mut seeds = meshes.to_vec();
    seeds.sort_by(|lhs, rhs| {
        rhs.triangles
            .len()
            .cmp(&lhs.triangles.len())
            .then(lhs.block.cmp(&rhs.block))
    });
    for seed in seeds {
        process_seed(&seed, meshes, &sweeps, options, &mut remaining, &mut result);
    }
    apply_fallback(options, &mut remaining, &mut result)?;
    report_unsupported(&remaining, &mut result);
    if result.brushes.len() > options.max_brushes {
        return Err(Error::Reconstruction(format!(
            "reconstruction produced {} brushes, exceeding --max-brushes {}",
            result.brushes.len(),
            options.max_brushes
        )));
    }
    if options.validate {
        validate_reconstruction(&mut result)?;
    }
    result.uv_max_error = max_uv_error(meshes);
    Ok(result)
}

pub(super) fn process_seed(
    seed: &VisualMesh,
    meshes: &[VisualMesh],
    sweeps: &HashMap<usize, Option<Sweep>>,
    options: &Options,
    remaining: &mut HashMap<usize, VisualMesh>,
    result: &mut Reconstruction,
) {
    if !remaining.contains_key(&seed.block) {
        return;
    }
    let Some(sweep) = sweeps.get(&seed.block).and_then(|sweep| sweep.as_ref()) else {
        return;
    };
    if let Some((brushes, report)) =
        exact_extrusion(std::slice::from_ref(seed), sweep, &options.skip_material)
    {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        result.used_shapes.insert(seed.block);
        remaining.remove(&seed.block);
        return;
    }
    let available: Vec<_> = meshes
        .iter()
        .filter(|mesh| remaining.contains_key(&mesh.block))
        .cloned()
        .collect();
    let group = group_open_shell_shapes(seed, sweep, &available, sweeps);
    if let Some((brushes, report)) = swept_shell(
        &group,
        sweep,
        options.shell_thickness,
        &options.skip_material,
    ) {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        for mesh in group {
            result.used_shapes.insert(mesh.block);
            remaining.remove(&mesh.block);
        }
        return;
    }
    if let Some((brushes, report)) =
        exact_extrusion(std::slice::from_ref(seed), sweep, &options.skip_material)
    {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        result.used_shapes.insert(seed.block);
        remaining.remove(&seed.block);
    }
}

pub(super) fn apply_fallback(
    options: &Options,
    remaining: &mut HashMap<usize, VisualMesh>,
    result: &mut Reconstruction,
) -> Result<(), Error> {
    if options.fallback == "skip" {
        return Ok(());
    }
    if options.fallback != "planar-prisms" {
        return Err(Error::Reconstruction(format!(
            "unknown fallback mode {:?}",
            options.fallback
        )));
    }
    let mut fallback_meshes: Vec<_> = remaining.values().cloned().collect();
    fallback_meshes.sort_by_key(|mesh| mesh.block);
    for mesh in fallback_meshes {
        match planar_fallback(&mesh, options.fallback_thickness, &options.skip_material) {
            Ok((brushes, report)) if !brushes.is_empty() => {
                result.brushes.extend(brushes);
                result.recognizers.push(report);
                result.used_shapes.insert(mesh.block);
                remaining.remove(&mesh.block);
            }
            Ok(_) => {}
            Err(error) => result.warnings.push(format!(
                "shape {} {:?}: fallback failed: {error}",
                mesh.block, mesh.name
            )),
        }
    }
    Ok(())
}

pub(super) fn report_unsupported(
    remaining: &HashMap<usize, VisualMesh>,
    result: &mut Reconstruction,
) {
    let mut unsupported: Vec<_> = remaining.values().collect();
    unsupported.sort_by_key(|mesh| mesh.block);
    for mesh in unsupported {
        result.warnings.push(format!(
            "shape {} {:?}: unsupported/unreconstructed ({} verts, {} triangles)",
            mesh.block,
            mesh.name,
            mesh.vertices.len(),
            mesh.triangles.len()
        ));
    }
}

pub(super) fn validate_reconstruction(result: &mut Reconstruction) -> Result<(), Error> {
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

pub(super) fn max_uv_error(meshes: &[VisualMesh]) -> f64 {
    meshes
        .iter()
        .filter(|mesh| mesh.uvs.is_some())
        .flat_map(|mesh| (0..mesh.triangles.len()).map(move |index| (mesh, index)))
        .filter_map(|(mesh, index)| triangle_projection(mesh, index))
        .map(|projection| projection.max_error)
        .fold(0.0, f64::max)
}
