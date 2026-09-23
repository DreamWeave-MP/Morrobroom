use super::*;

fn cluster_values(mut values: Vec<f64>, epsilon: f64) -> Vec<f64> {
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

fn nearest_layer(value: f64, layers: &[f64]) -> usize {
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

fn unique_edges(mesh: &VisualMesh) -> Vec<(usize, usize)> {
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

fn candidate_directions(mesh: &VisualMesh, edges: &[(usize, usize)]) -> Vec<P3> {
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

fn analyze_sweep(mesh: &VisualMesh, edges: &[(usize, usize)], direction: P3) -> Option<Sweep> {
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

pub(in crate::nif2map) fn detect_sweep(mesh: &VisualMesh) -> Option<Sweep> {
    let edges = unique_edges(mesh);
    candidate_directions(mesh, &edges)
        .into_iter()
        .filter_map(|direction| analyze_sweep(mesh, &edges, direction))
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

pub(in crate::nif2map) fn compatible_sweep(lhs: &Sweep, rhs: &Sweep) -> bool {
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

fn project(point: P3, sweep: &Sweep) -> Coord<f64> {
    Coord {
        x: point.dot(sweep.u),
        y: point.dot(sweep.v),
    }
}

pub(in crate::nif2map) fn unproject(point: Coord<f64>, t: f64, sweep: &Sweep) -> P3 {
    sweep.u * point.x + sweep.v * point.y + sweep.direction * t
}

pub(in crate::nif2map) fn collect_caps(
    meshes: &[&VisualMesh],
    sweep: &Sweep,
) -> (Vec<Cap>, Vec<Cap>) {
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

pub(in crate::nif2map) fn collect_segments(
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
                if (b.x - a.x).hypot(b.y - a.y) <= WELD_EPSILON {
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

pub(in crate::nif2map) fn profile_cycles<S: Borrow<Segment>>(
    segments: &[S],
) -> Option<Vec<Polygon<f64>>> {
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

fn profile_area_signature(cycles: &[Polygon<f64>]) -> Vec<f64> {
    let mut areas: Vec<_> = cycles.iter().map(polygon_area).collect();
    areas.sort_by(f64::total_cmp);
    areas
}

pub(in crate::nif2map) type ProfileValidation = (Vec<Segment>, Vec<Polygon<f64>>, f64, f64, f64);

pub(in crate::nif2map) fn validate_profile(
    meshes: &[&VisualMesh],
    sweep: &Sweep,
) -> Option<ProfileValidation> {
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

fn projected_layer_sets(meshes: &[&VisualMesh], sweep: &Sweep) -> Vec<HashSet<Q>> {
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

pub(in crate::nif2map) fn exact_layer_invariance(meshes: &[&VisualMesh], sweep: &Sweep) -> bool {
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
