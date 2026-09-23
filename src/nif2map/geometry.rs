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
    let mut clusters = Vec::new();
    let mut sum = first;
    let mut count = 1_usize;
    for value in values.into_iter().skip(1) {
        let center = sum / count as f64;
        if (value - center).abs() <= epsilon {
            sum += value;
            count += 1;
        } else {
            clusters.push(sum / count as f64);
            sum = value;
            count = 1;
        }
    }
    clusters.push(sum / count as f64);
    clusters
}

pub(super) fn nearest_layer(value: f64, layers: &[f64]) -> usize {
    let insertion = layers.partition_point(|layer| *layer < value);
    match insertion {
        0 => 0,
        index if index == layers.len() => layers.len() - 1,
        index => {
            let lower_distance = value - layers[index - 1];
            let upper_distance = layers[index] - value;
            if lower_distance <= upper_distance {
                index - 1
            } else {
                index
            }
        }
    }
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

pub(super) fn candidate_directions(mesh: &VisualMesh, edges: &[(usize, usize)]) -> Vec<P3> {
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
    for &(a, b) in edges {
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

pub(super) fn analyze_sweep(
    mesh: &VisualMesh,
    edges: &[(usize, usize)],
    direction: P3,
) -> Option<Sweep> {
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
    for &(a, b) in edges {
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
    let edges = unique_edges(mesh);
    candidate_directions(mesh, &edges)
        .into_iter()
        .filter_map(|direction| analyze_sweep(mesh, &edges, direction))
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

pub(super) fn collect_caps(meshes: &[&VisualMesh], sweep: &Sweep) -> (Vec<Cap>, Vec<Cap>) {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut low = Vec::new();
    let mut high = Vec::new();
    for mesh in meshes {
        let mesh = *mesh;
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
    meshes: &[&VisualMesh],
    sweep: &Sweep,
    layer_index: Option<usize>,
) -> Vec<Segment> {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut collected: HashMap<(Q, Q), Segment> = HashMap::new();
    for mesh in meshes {
        let mesh = *mesh;
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

pub(super) fn profile_cycles<S: Borrow<Segment>>(segments: &[S]) -> Option<Vec<Polygon<f64>>> {
    let mut adjacency: HashMap<Q, HashSet<Q>> = HashMap::new();
    let mut points: HashMap<Q, Coord<f64>> = HashMap::new();
    let mut edges = HashSet::new();
    for segment in segments {
        let segment = segment.borrow();
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

pub(super) fn validate_profile(meshes: &[&VisualMesh], sweep: &Sweep) -> Option<ProfileValidation> {
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

pub(super) fn projected_layer_sets(meshes: &[&VisualMesh], sweep: &Sweep) -> Vec<HashSet<Q>> {
    let extent = (sweep.t_max - sweep.t_min).abs().max(1.0);
    let tolerance = LAYER_EPSILON.max(extent * 1e-6) * 4.0;
    let mut result = vec![HashSet::new(); sweep.layers.len()];
    for mesh in meshes {
        let mesh = *mesh;
        for vertex in &mesh.vertices {
            let layer = nearest_layer(vertex.dot(sweep.direction), &sweep.layers);
            if (vertex.dot(sweep.direction) - sweep.layers[layer]).abs() <= tolerance {
                result[layer].insert(qkey(project(*vertex, sweep)));
            }
        }
    }
    result
}

pub(super) fn exact_layer_invariance(meshes: &[&VisualMesh], sweep: &Sweep) -> bool {
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

fn decomposition_preserves_target(target: &Polygon<f64>, pieces: &[Polygon<f64>]) -> bool {
    !pieces.is_empty()
        && pieces.iter().all(|piece| {
            piece.interiors().is_empty() && polygon_is_convex(piece) && target.covers(piece)
        })
        && (pieces.iter().map(polygon_area).sum::<f64>() - polygon_area(target)).abs()
            <= 1e-6_f64.max(polygon_area(target) * 1e-8)
}

fn validated_triangulation_decomposition(
    target: &Polygon<f64>,
) -> Result<Vec<Polygon<f64>>, Error> {
    let floor = triangulation_decomposition(target);
    if decomposition_preserves_target(target, &floor) {
        Ok(floor)
    } else {
        Err(Error::Reconstruction(
            "earcut could not convex-decompose 2D target without losing geometry".into(),
        ))
    }
}

fn ring_vertices(polygon: &Polygon<f64>) -> Vec<Coord<f64>> {
    let mut vertices = polygon.exterior().0.clone();
    if vertices.first() == vertices.last() {
        vertices.pop();
    }
    vertices.push(vertices[0]);
    vertices
}

fn directed_ring_edge(vertices: &[Coord<f64>], from: Q, to: Q) -> Option<usize> {
    (0..vertices.len().saturating_sub(1))
        .find(|&index| qkey(vertices[index]) == from && qkey(vertices[index + 1]) == to)
}

fn path_without_edge(vertices: &[Coord<f64>], edge: usize) -> Vec<Coord<f64>> {
    let count = vertices.len() - 1;
    let mut path = Vec::with_capacity(count.saturating_sub(1));
    let mut index = (edge + 1) % count;
    loop {
        path.push(vertices[index]);
        if index == edge {
            break;
        }
        index = (index + 1) % count;
    }
    path
}

type SharedBoundaryEdge = ((Q, Q), [Coord<f64>; 2]);
const MAX_LOCAL_REPAIR_REGION_PIECES: usize = 16;
const MAX_LOCAL_REPAIR_REGION_CANDIDATES: usize = 4_096;

fn shared_boundary_edge(lhs: &Polygon<f64>, rhs: &Polygon<f64>) -> Option<SharedBoundaryEdge> {
    let lhs_vertices = ring_vertices(lhs);
    let rhs_vertices = ring_vertices(rhs);
    for lhs_edge in lhs_vertices.windows(2) {
        let start = qkey(lhs_edge[0]);
        let end = qkey(lhs_edge[1]);
        if start == end {
            continue;
        }
        if rhs_vertices
            .windows(2)
            .any(|edge| qkey(edge[0]) == end && qkey(edge[1]) == start)
        {
            return Some(((start, end), [lhs_edge[0], lhs_edge[1]]));
        }
    }
    None
}

fn splice_adjacent_polygons(lhs: &Polygon<f64>, rhs: &Polygon<f64>) -> Option<Polygon<f64>> {
    if !lhs.interiors().is_empty() || !rhs.interiors().is_empty() {
        return None;
    }
    let lhs_vertices = ring_vertices(lhs);
    let rhs_vertices = ring_vertices(rhs);
    for edge in lhs_vertices.windows(2) {
        let first = qkey(edge[0]);
        let second = qkey(edge[1]);
        if first == second {
            continue;
        }
        let Some(lhs_edge) = directed_ring_edge(&lhs_vertices, first, second) else {
            continue;
        };
        let Some(rhs_edge) = directed_ring_edge(&rhs_vertices, second, first) else {
            continue;
        };
        let mut coordinates = path_without_edge(&lhs_vertices, lhs_edge);
        coordinates.extend(
            path_without_edge(&rhs_vertices, rhs_edge)
                .into_iter()
                .skip(1),
        );
        let coordinates = remove_collinear(coordinates);
        if coordinates.len() < 3 {
            return None;
        }
        let merged = poly(coordinates);
        return (polygon_area(&merged) > 1e-9).then_some(merged);
    }
    None
}

fn covers_with_weld_tolerance(polygon: &Polygon<f64>, point: Coord<f64>) -> bool {
    let point_geometry = Point::from(point);
    polygon.covers(&point_geometry)
        || polygon
            .exterior()
            .0
            .windows(2)
            .any(|edge| point_segment_distance(edge[0], edge[1], point) <= WELD_EPSILON)
}

fn valid_convex_merge(lhs: &Polygon<f64>, rhs: &Polygon<f64>, merged: &Polygon<f64>) -> bool {
    let source_area = polygon_area(lhs) + polygon_area(rhs);
    let area_tolerance =
        1e-6_f64.max((ring_length(lhs.exterior()) + ring_length(rhs.exterior())) * WELD_EPSILON);
    lhs.interiors().is_empty()
        && rhs.interiors().is_empty()
        && merged.interiors().is_empty()
        && polygon_is_convex(lhs)
        && polygon_is_convex(rhs)
        && polygon_is_convex(merged)
        && [lhs, rhs].into_iter().all(|piece| {
            piece
                .exterior()
                .0
                .iter()
                .all(|point| covers_with_weld_tolerance(merged, *point))
        })
        && (polygon_area(merged) - source_area).abs() <= area_tolerance
}

pub(super) fn flip_shared_diagonal(
    lhs: &Polygon<f64>,
    rhs: &Polygon<f64>,
) -> Option<[Polygon<f64>; 2]> {
    let ((start_key, end_key), [start, end]) = shared_boundary_edge(lhs, rhs)?;
    let lhs_vertices = ring_vertices(lhs);
    let rhs_vertices = ring_vertices(rhs);
    let lhs_other: Vec<_> = lhs_vertices[..lhs_vertices.len() - 1]
        .iter()
        .copied()
        .filter(|point| {
            let key = qkey(*point);
            key != start_key && key != end_key
        })
        .collect();
    let rhs_other: Vec<_> = rhs_vertices[..rhs_vertices.len() - 1]
        .iter()
        .copied()
        .filter(|point| {
            let key = qkey(*point);
            key != start_key && key != end_key
        })
        .collect();
    let [lhs_opposite] = lhs_other.as_slice() else {
        return None;
    };
    let [rhs_opposite] = rhs_other.as_slice() else {
        return None;
    };
    let merged = splice_adjacent_polygons(lhs, rhs)?;
    if !valid_convex_merge(lhs, rhs, &merged) || merged.exterior().0.len() != 5 {
        return None;
    }
    let flipped = [
        poly(vec![*lhs_opposite, *rhs_opposite, start]),
        poly(vec![*rhs_opposite, *lhs_opposite, end]),
    ];
    let merged_area = polygon_area(&merged);
    let flipped_area: f64 = flipped.iter().map(polygon_area).sum();
    if flipped.iter().any(|piece| {
        polygon_area(piece) <= 1e-9 || !polygon_is_convex(piece) || !merged.covers(piece)
    }) || (flipped_area - merged_area).abs()
        > 1e-6_f64.max(
            (ring_length(merged.exterior())
                + ring_length(lhs.exterior())
                + ring_length(rhs.exterior()))
                * WELD_EPSILON,
        )
    {
        return None;
    }
    Some(flipped)
}

fn stitch_piece_region(pieces: &[&Polygon<f64>]) -> Option<Polygon<f64>> {
    if pieces.len() < 2 || pieces.iter().any(|piece| !polygon_is_convex(piece)) {
        return None;
    }
    let mut boundary = HashMap::<(Q, Q), (Coord<f64>, Coord<f64>)>::new();
    for piece in pieces {
        let vertices = ring_vertices(piece);
        for edge in vertices.windows(2) {
            let start = qkey(edge[0]);
            let end = qkey(edge[1]);
            if start == end {
                continue;
            }
            if boundary.remove(&(end, start)).is_none()
                && boundary.insert((start, end), (edge[0], edge[1])).is_some()
            {
                return None;
            }
        }
    }
    if boundary.len() < 3 {
        return None;
    }
    let mut outgoing = HashMap::<Q, (Q, Coord<f64>)>::new();
    for (&(start, end), &(point, _)) in &boundary {
        if outgoing.insert(start, (end, point)).is_some() {
            return None;
        }
    }
    let (&start_key, _) = outgoing.iter().next()?;
    let mut coordinates = Vec::with_capacity(boundary.len());
    let mut current = start_key;
    let mut visited = HashSet::new();
    loop {
        let (next, point) = *outgoing.get(&current)?;
        if !visited.insert((current, next)) {
            return None;
        }
        coordinates.push(point);
        current = next;
        if current == start_key {
            break;
        }
        if visited.len() >= boundary.len() {
            return None;
        }
    }
    if visited.len() != boundary.len() {
        return None;
    }
    let merged = poly(remove_collinear(coordinates));
    let expected_area: f64 = pieces.iter().map(|piece| polygon_area(piece)).sum();
    let area_tolerance = 1e-6_f64.max(
        pieces
            .iter()
            .map(|piece| ring_length(piece.exterior()))
            .sum::<f64>()
            * WELD_EPSILON,
    );
    (merged.interiors().is_empty()
        && pieces.iter().all(|piece| {
            piece
                .exterior()
                .0
                .iter()
                .all(|point| covers_with_weld_tolerance(&merged, *point))
        })
        && (polygon_area(&merged) - expected_area).abs() <= area_tolerance)
        .then_some(merged)
}

fn polygon_signature(polygon: &Polygon<f64>) -> Vec<Q> {
    let mut vertices: Vec<_> = polygon
        .exterior()
        .0
        .iter()
        .take(polygon.exterior().0.len().saturating_sub(1))
        .map(|point| qkey(*point))
        .collect();
    vertices.sort_unstable();
    vertices.dedup();
    vertices
}

fn pair_signature(lhs: &Polygon<f64>, rhs: &Polygon<f64>, operation: u8) -> (Vec<Q>, Vec<Q>, u8) {
    let lhs_signature = polygon_signature(lhs);
    let rhs_signature = polygon_signature(rhs);
    if lhs_signature <= rhs_signature {
        (lhs_signature, rhs_signature, operation)
    } else {
        (rhs_signature, lhs_signature, operation)
    }
}

fn replace_piece_pair(
    pieces: &mut Vec<Polygon<f64>>,
    first: usize,
    second: usize,
    replacements: impl IntoIterator<Item = Polygon<f64>>,
) {
    let low = first.min(second);
    let high = first.max(second);
    pieces.remove(high);
    pieces.remove(low);
    for (offset, replacement) in replacements.into_iter().enumerate() {
        pieces.insert(low + offset, replacement);
    }
}

fn try_pair_repair<F>(
    pieces: &mut Vec<Polygon<f64>>,
    failed_index: usize,
    neighbor_index: usize,
    tried: &mut HashSet<(Vec<Q>, Vec<Q>, u8)>,
    emit: &mut F,
) -> bool
where
    F: FnMut(&Polygon<f64>) -> Result<Brush, Error>,
{
    let failed = &pieces[failed_index];
    let neighbor = &pieces[neighbor_index];
    if shared_boundary_edge(failed, neighbor).is_none() {
        return false;
    }
    let flip_key = pair_signature(failed, neighbor, 0);
    let flipped = tried
        .insert(flip_key)
        .then(|| flip_shared_diagonal(failed, neighbor))
        .flatten();
    if let Some(flipped) = flipped
        && flipped.iter().all(|piece| {
            emit(piece)
                .and_then(|brush| {
                    validate_brush(&brush)?;
                    Ok(())
                })
                .is_ok()
        })
    {
        replace_piece_pair(pieces, failed_index, neighbor_index, flipped);
        return true;
    }
    let merge_key = pair_signature(failed, neighbor, 1);
    let merged = tried
        .insert(merge_key)
        .then(|| splice_adjacent_polygons(failed, neighbor))
        .flatten();
    if let Some(merged) = merged.filter(|merged| valid_convex_merge(failed, neighbor, merged))
        && emit(&merged)
            .and_then(|brush| {
                validate_brush(&brush)?;
                Ok(())
            })
            .is_ok()
    {
        replace_piece_pair(pieces, failed_index, neighbor_index, [merged]);
        return true;
    }
    false
}

fn try_grow_local_repair_region<F>(
    pieces: &mut Vec<Polygon<f64>>,
    failed_index: usize,
    tried_regions: &mut HashSet<Vec<usize>>,
    emit: &mut F,
) -> bool
where
    F: FnMut(&Polygon<f64>) -> Result<Brush, Error>,
{
    let mut queue = std::collections::VecDeque::from([vec![failed_index]]);
    while let Some(mut region) = queue.pop_front() {
        if region.len() >= MAX_LOCAL_REPAIR_REGION_PIECES
            || tried_regions.len() >= MAX_LOCAL_REPAIR_REGION_CANDIDATES
        {
            continue;
        }
        let adjacent: Vec<_> = (0..pieces.len())
            .filter(|candidate| !region.contains(candidate))
            .filter(|candidate| {
                region.iter().any(|&member| {
                    shared_boundary_edge(&pieces[member], &pieces[*candidate]).is_some()
                })
            })
            .collect();
        for candidate in adjacent {
            region.push(candidate);
            region.sort_unstable();
            if tried_regions.insert(region.clone()) {
                let region_pieces: Vec<_> = region.iter().map(|&index| &pieces[index]).collect();
                if let Some(region_polygon) = stitch_piece_region(&region_pieces) {
                    let replacements = if polygon_is_convex(&region_polygon) {
                        vec![region_polygon.clone()]
                    } else {
                        triangulation_decomposition(&region_polygon)
                    };
                    if decomposition_preserves_target(&region_polygon, &replacements)
                        && replacements.iter().all(|piece| {
                            emit(piece)
                                .and_then(|brush| {
                                    validate_brush(&brush)?;
                                    Ok(())
                                })
                                .is_ok()
                        })
                    {
                        let first = region[0];
                        for &index in region.iter().rev() {
                            pieces.remove(index);
                        }
                        for (offset, replacement) in replacements.into_iter().enumerate() {
                            pieces.insert(first + offset, replacement);
                        }
                        return true;
                    }
                }
                queue.push_back(region.clone());
            }
            region.retain(|&index| index != candidate);
        }
    }
    false
}

pub(super) fn emit_with_local_repairs<F>(
    mut pieces: Vec<Polygon<f64>>,
    mut emit: F,
) -> Result<Vec<Brush>, Error>
where
    F: FnMut(&Polygon<f64>) -> Result<Brush, Error>,
{
    let mut tried = HashSet::<(Vec<Q>, Vec<Q>, u8)>::new();
    let mut tried_regions = HashSet::<Vec<usize>>::new();
    loop {
        let mut brushes = Vec::with_capacity(pieces.len());
        let mut failed_piece = None;
        let mut emission_error = None;
        for (index, piece) in pieces.iter().enumerate() {
            match emit(piece).and_then(|brush| {
                validate_brush(&brush)?;
                Ok(brush)
            }) {
                Ok(brush) => brushes.push(brush),
                Err(error) => {
                    failed_piece = Some(index);
                    emission_error = Some(error);
                    break;
                }
            }
        }
        let Some(failed_index) = failed_piece else {
            return Ok(brushes);
        };
        let mut repaired = false;
        for neighbor_index in 0..pieces.len() {
            if neighbor_index != failed_index
                && try_pair_repair(
                    &mut pieces,
                    failed_index,
                    neighbor_index,
                    &mut tried,
                    &mut emit,
                )
            {
                repaired = true;
                tried_regions.clear();
                break;
            }
        }
        if !repaired {
            repaired = try_grow_local_repair_region(
                &mut pieces,
                failed_index,
                &mut tried_regions,
                &mut emit,
            );
            if repaired {
                tried_regions.clear();
            }
        }
        if !repaired {
            let error = emission_error.expect("failed piece always captures its error");
            return Err(Error::Reconstruction(format!(
                "{error}; local repair tried {} edge operations and {} connected regions",
                tried.len(),
                tried_regions.len()
            )));
        }
    }
}

fn convex_hull_merge(lhs: &Polygon<f64>, rhs: &Polygon<f64>) -> Polygon<f64> {
    let points = lhs
        .exterior()
        .0
        .iter()
        .chain(rhs.exterior().0.iter())
        .copied()
        .collect::<Vec<_>>();
    LineString::from(points).convex_hull()
}

pub(super) fn greedy_convex_merge(polygons: Vec<Polygon<f64>>) -> Vec<Polygon<f64>> {
    let mut active: Vec<Option<Polygon<f64>>> = polygons.into_iter().map(Some).collect();
    loop {
        let mut edges: HashMap<(Q, Q), Vec<usize>> = HashMap::new();
        for (index, polygon) in active.iter().enumerate() {
            let Some(polygon) = polygon else {
                continue;
            };
            for edge in polygon_edge_keys(polygon) {
                edges.entry(edge).or_default().push(index);
            }
        }
        let mut candidates = std::collections::BTreeSet::new();
        for indexes in edges.values() {
            for (offset, &first) in indexes.iter().enumerate() {
                for &second in indexes.iter().skip(offset + 1) {
                    if first == second {
                        continue;
                    }
                    candidates.insert(if first < second {
                        (first, second)
                    } else {
                        (second, first)
                    });
                }
            }
        }
        let mut best: Option<(f64, usize, usize, Polygon<f64>)> = None;
        for (i, j) in candidates {
            let (Some(lhs), Some(rhs)) = (active[i].as_ref(), active[j].as_ref()) else {
                continue;
            };
            // Candidate pairs come from the same complete quantized edge;
            // the hull fallback only repairs coordinate/winding noise in the
            // ring splice and is still admitted only by the local invariants.
            let merged = splice_adjacent_polygons(lhs, rhs)
                .filter(|merged| valid_convex_merge(lhs, rhs, merged))
                .or_else(|| {
                    let merged = convex_hull_merge(lhs, rhs);
                    valid_convex_merge(lhs, rhs, &merged).then_some(merged)
                });
            let Some(merged) = merged else {
                continue;
            };
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
        let Some((_, i, j, merged)) = best else {
            break;
        };
        active[i] = None;
        active[j] = None;
        active.push(Some(merged));
    }
    active.into_iter().flatten().collect()
}

fn polygon_edge_keys(polygon: &Polygon<f64>) -> Vec<(Q, Q)> {
    polygon
        .exterior()
        .0
        .windows(2)
        .map(|edge| {
            let first = qkey(edge[0]);
            let second = qkey(edge[1]);
            if first <= second {
                (first, second)
            } else {
                (second, first)
            }
        })
        .filter(|(first, second)| first != second)
        .collect()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PlaneBucket {
    normal: [i64; 3],
    distance: i64,
}

const PLANE_BUCKET_SIZE: f64 = 1e-2;

fn plane_bucket(normal: P3, distance: f64) -> PlaneBucket {
    PlaneBucket {
        normal: [normal.x, normal.y, normal.z]
            .map(|component| (component / PLANE_BUCKET_SIZE).round() as i64),
        distance: (distance / PLANE_BUCKET_SIZE).round() as i64,
    }
}

fn neighboring_plane_buckets(bucket: PlaneBucket) -> [PlaneBucket; 81] {
    let mut buckets = [bucket; 81];
    let mut index = 0;
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                for distance in -1..=1 {
                    buckets[index] = PlaneBucket {
                        normal: [
                            bucket.normal[0] + x,
                            bucket.normal[1] + y,
                            bucket.normal[2] + z,
                        ],
                        distance: bucket.distance + distance,
                    };
                    index += 1;
                }
            }
        }
    }
    buckets
}

pub(super) fn planar_fallback(
    mesh: &VisualMesh,
    thickness: f64,
    skip: &str,
) -> Result<(Vec<Brush>, RecognizerReport), Error> {
    // Coplanar triangles may belong to different UV charts. Merge only when
    // one affine projection reproduces the candidate triangle within tolerance.
    let mut groups: Vec<(P3, f64, Vec<usize>, Option<Projection>)> = Vec::new();
    let mut plane_buckets: HashMap<PlaneBucket, Vec<usize>> = HashMap::new();
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
        let bucket = plane_bucket(normal, distance);
        let mut matching_group = None;
        for neighboring in neighboring_plane_buckets(bucket) {
            for &group_index in plane_buckets.get(&neighboring).into_iter().flatten() {
                let (candidate, candidate_distance, _, group_projection) = &groups[group_index];
                if candidate.dot(normal).abs() >= 0.99999
                    && (*candidate_distance - distance).abs() <= 1e-3
                    && (mesh.uvs.is_none()
                        || can_merge_uv_triangle(
                            mesh,
                            triangle_index,
                            projection.as_ref(),
                            group_projection.as_ref(),
                        ))
                {
                    matching_group = Some(group_index);
                    break;
                }
            }
            if matching_group.is_some() {
                break;
            }
        }
        if let Some(group_index) = matching_group {
            groups[group_index].2.push(triangle_index);
        } else {
            let group_index = groups.len();
            groups.push((normal, distance, vec![triangle_index], projection));
            plane_buckets.entry(bucket).or_default().push(group_index);
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
    let mut triangle_polygons = Vec::new();
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
            triangle_polygons.push(polygon);
        }
    }
    if triangle_polygons.is_empty() {
        return Ok(Vec::new());
    }
    let polygons =
        indexed_boundary_components(mesh, triangle_indices, origin, tangent_u, tangent_v);
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
    let regions = if triangle_polygons.len() == 1 {
        triangle_polygons
    } else {
        polygons.unwrap_or_else(|| union_polygons(&triangle_polygons).0)
    };
    for region_polygon in regions {
        let pieces = if region_polygon.interiors().is_empty() && polygon_is_convex(&region_polygon)
        {
            vec![region_polygon.clone()]
        } else {
            decompose(&region_polygon)
                .or_else(|_| validated_triangulation_decomposition(&region_polygon))?
        };
        if let Ok(region_brushes) = emit_fallback_pieces(&context, &pieces) {
            brushes.extend(region_brushes);
        } else {
            let floor = validated_triangulation_decomposition(&region_polygon)?;
            brushes.extend(emit_fallback_pieces(&context, &floor)?);
        }
    }
    Ok(brushes)
}

fn emit_fallback_pieces(
    context: &FallbackContext<'_>,
    pieces: &[Polygon<f64>],
) -> Result<Vec<Brush>, Error> {
    emit_with_local_repairs(pieces.to_vec(), |piece| {
        fallback_piece(context, piece).unwrap_or_else(|| {
            Err(Error::Reconstruction(
                "fallback piece collapsed during brush emission".into(),
            ))
        })
    })
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
    let (front_face, front_plane) = match emit_face_with_plane(
        &front,
        context.authored_normal,
        &context.mesh.material,
        source_projection,
    ) {
        Ok(face) => face,
        Err(error) => return Some(Err(error)),
    };
    let (back_face, back_plane) =
        match emit_face_with_plane(&back, -context.authored_normal, context.skip, None) {
            Ok(face) => face,
            Err(error) => return Some(Err(error)),
        };
    let mut faces = vec![front_face, back_face];
    let mut planes = vec![front_plane, back_plane];
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
        let normal = match side_normal {
            Ok(normal) => normal,
            Err(error) => return Some(Err(error)),
        };
        let (face, plane) = match emit_face_with_plane(&quad, normal, context.skip, None) {
            Ok(face) => face,
            Err(error) => return Some(Err(error)),
        };
        faces.push(face);
        planes.push(plane);
    }
    Some(Ok(Brush {
        faces,
        planes,
        kind: "planar-prism-fallback".into(),
        shapes: vec![context.mesh.block],
        ..Default::default()
    }))
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

pub(super) fn reconstruct(
    meshes: &[VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut partitions: BTreeMap<(ScopeId, NifState), Vec<&VisualMesh>> = BTreeMap::new();
    for mesh in meshes {
        partitions
            .entry((mesh.scope, mesh.nif_state.clone()))
            .or_default()
            .push(mesh);
    }
    let partitions: Vec<_> = partitions.into_iter().collect();
    let partials: Vec<_> = partitions
        .into_iter()
        .map(|((scope, nif_state), partition)| {
            let mut partial = reconstruct_partition(&partition, options)?;
            for brush in &mut partial.brushes {
                brush.scope = scope;
                brush.nif_state = nif_state.clone();
            }
            Ok::<_, Error>((partial, scope, nif_state))
        })
        .collect();
    let mut result = Reconstruction::default();
    for partial in partials {
        let (partial, _scope, _nif_state) = partial?;
        result.brushes.extend(partial.brushes);
        result.used_shapes.extend(partial.used_shapes);
        result.recognizers.extend(partial.recognizers);
        result.warnings.extend(partial.warnings);
        result.uv_max_error = result.uv_max_error.max(partial.uv_max_error);
        result.timings.add_assign(partial.timings);
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
    meshes: &[&VisualMesh],
    options: &Options,
) -> Result<Reconstruction, Error> {
    let mut result = Reconstruction::default();
    let mut remaining: HashSet<usize> = meshes.iter().map(|mesh| mesh.block).collect();
    let sweep_started = Instant::now();
    let sweeps: HashMap<usize, Option<Sweep>> = meshes
        .iter()
        .map(|mesh| (mesh.block, detect_sweep(mesh)))
        .collect();
    result.timings.sweep_analysis = sweep_started.elapsed();
    let mut seeds = meshes.to_vec();
    seeds.sort_by(|lhs, rhs| {
        rhs.triangles
            .len()
            .cmp(&lhs.triangles.len())
            .then(lhs.block.cmp(&rhs.block))
    });
    let structural_started = Instant::now();
    for seed in seeds {
        process_seed(seed, meshes, &sweeps, options, &mut remaining, &mut result);
    }
    result.timings.structural_recognition = structural_started.elapsed();
    let fallback_started = Instant::now();
    apply_fallback(options, meshes, &mut remaining, &mut result)?;
    result.timings.planar_fallback = fallback_started.elapsed();
    report_unsupported(meshes, &remaining, &mut result);
    if result.brushes.len() > options.max_brushes {
        return Err(Error::Reconstruction(format!(
            "reconstruction produced {} brushes, exceeding --max-brushes {}",
            result.brushes.len(),
            options.max_brushes
        )));
    }
    if options.validate {
        let validation_started = Instant::now();
        validate_reconstruction(&mut result)?;
        result.timings.brush_validation = validation_started.elapsed();
    }
    let uv_started = Instant::now();
    result.uv_max_error = max_uv_error(meshes);
    result.timings.uv_diagnostics = uv_started.elapsed();
    Ok(result)
}

pub(super) fn process_seed(
    seed: &VisualMesh,
    meshes: &[&VisualMesh],
    sweeps: &HashMap<usize, Option<Sweep>>,
    options: &Options,
    remaining: &mut HashSet<usize>,
    result: &mut Reconstruction,
) {
    if !remaining.contains(&seed.block) {
        return;
    }
    let Some(sweep) = sweeps.get(&seed.block).and_then(|sweep| sweep.as_ref()) else {
        return;
    };
    let seed_group = [seed];
    if let Some((brushes, report)) = exact_extrusion(&seed_group, sweep, &options.skip_material) {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        result.used_shapes.insert(seed.block);
        remaining.remove(&seed.block);
        return;
    }
    let group = group_open_shell_shapes(seed, sweep, remaining, meshes, sweeps);
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
    }
}

pub(super) fn apply_fallback(
    options: &Options,
    meshes: &[&VisualMesh],
    remaining: &mut HashSet<usize>,
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
    let mut fallback_meshes: Vec<_> = meshes
        .iter()
        .copied()
        .filter(|mesh| remaining.contains(&mesh.block))
        .collect();
    fallback_meshes.sort_by_key(|mesh| mesh.block);
    let mut fallback_results: Vec<_> = fallback_meshes
        .iter()
        .map(|mesh| {
            (
                mesh.block,
                planar_fallback(mesh, options.fallback_thickness, &options.skip_material),
            )
        })
        .collect();
    fallback_results.sort_by_key(|(block, _)| *block);
    for (block, fallback) in fallback_results {
        let mesh = fallback_meshes
            .binary_search_by_key(&block, |mesh| mesh.block)
            .ok()
            .and_then(|index| fallback_meshes.get(index))
            .expect("fallback result should have a source mesh");
        match fallback {
            Ok((brushes, report)) if !brushes.is_empty() => {
                result.brushes.extend(brushes);
                result.recognizers.push(report);
                result.used_shapes.insert(block);
                remaining.remove(&block);
            }
            Ok(_) => {}
            Err(error) => result.warnings.push(format!(
                "shape {} {:?}: fallback failed: {error}",
                block, mesh.name
            )),
        }
    }
    Ok(())
}

pub(super) fn report_unsupported(
    meshes: &[&VisualMesh],
    remaining: &HashSet<usize>,
    result: &mut Reconstruction,
) {
    let mut unsupported: Vec<_> = meshes
        .iter()
        .copied()
        .filter(|mesh| remaining.contains(&mesh.block))
        .collect();
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

pub(super) fn max_uv_error(meshes: &[&VisualMesh]) -> f64 {
    meshes
        .iter()
        .filter(|mesh| mesh.uvs.is_some())
        .flat_map(|mesh| (0..mesh.triangles.len()).map(move |index| (mesh, index)))
        .filter_map(|(mesh, index)| triangle_projection(mesh, index))
        .map(|projection| projection.max_error)
        .fold(0.0, f64::max)
}
