use super::*;

pub(in crate::nif2map) fn triangulation_decomposition(target: &Polygon<f64>) -> Vec<Polygon<f64>> {
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

pub(in crate::nif2map) fn decomposition_preserves_target(
    target: &Polygon<f64>,
    pieces: &[Polygon<f64>],
) -> bool {
    !pieces.is_empty()
        && pieces.iter().all(|piece| {
            piece.interiors().is_empty() && polygon_is_convex(piece) && target.covers(piece)
        })
        && (pieces.iter().map(polygon_area).sum::<f64>() - polygon_area(target)).abs()
            <= 1e-6_f64.max(polygon_area(target) * 1e-8)
}

pub(in crate::nif2map) fn validated_triangulation_decomposition(
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

type SharedBoundaryEdge = ((Q, Q), [Coord<f64>; 2]);
const MAX_LOCAL_REPAIR_REGION_PIECES: usize = 16;
const MAX_LOCAL_REPAIR_REGION_CANDIDATES: usize = 4_096;

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

pub(in crate::nif2map) fn flip_shared_diagonal(
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

pub(in crate::nif2map) fn emit_with_local_repairs<F>(
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

pub(in crate::nif2map) fn greedy_convex_merge(polygons: Vec<Polygon<f64>>) -> Vec<Polygon<f64>> {
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
