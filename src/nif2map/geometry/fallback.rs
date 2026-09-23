use super::*;

fn canonical_plane(normal: P3, distance: f64) -> (P3, f64) {
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

pub(in crate::nif2map) fn planar_fallback(
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

fn fallback_group(
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

struct FallbackContext<'a> {
    mesh: &'a VisualMesh,
    projection: Option<&'a Projection>,
    origin: P3,
    tangent_u: P3,
    tangent_v: P3,
    authored_normal: P3,
    thickness: f64,
    skip: &'a str,
}

fn fallback_piece(
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
    let (front_face, front_plane) = match emit_face_with_plane(
        &front,
        context.authored_normal,
        &context.mesh.material,
        context.projection,
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
