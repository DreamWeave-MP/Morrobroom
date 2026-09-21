//! Native, batch-oriented replacement for the experimental `nif2map.py` tool.
//!
//! The public contract intentionally follows the prototype: visual
//! `NiTriShape` geometry is authoritative, collision-node descendants are
//! ignored by default, reconstruction is conservative, and every emitted
//! brush is convex.  The implementation keeps geometry in compact `f64`
//! structs and processes independent files in parallel; no Python or `NumPy`
//! runtime is involved.
#![allow(
    clippy::cast_precision_loss,
    reason = "NIF geometry uses bounded mesh cardinalities and world coordinates."
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "The prototype quantizes bounded geometry into reconstruction grid keys."
)]

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use geo::{
    Area, BooleanOps, Buffer, Contains, ConvexHull, Coord, Covers, HausdorffDistance, Intersects,
    LineString, MultiPolygon, Point, Polygon, TriangulateEarcut, Winding,
    algorithm::{
        buffer::{BufferStyle, LineJoin},
        unary_union,
    },
    centroid::Centroid,
};
use nalgebra::Matrix3;
use rayon::prelude::*;
use serde::Serialize;
use tes3::nif::{
    NiKey, NiNode, NiStream, NiTexturingProperty, NiTriShape, NiTriShapeData, RootCollisionNode,
    TextureMap, TextureSource,
};

const WELD_EPSILON: f64 = 1e-3;
const LAYER_EPSILON: f64 = 1e-3;
const COLLINEAR_SINE_EPSILON: f64 = 1e-6;
const DIRECTION_DOT_EPSILON: f64 = 0.9995;
const GEOMETRY_EPSILON: f64 = 1e-5;
const FACE_TRIPLET_QUALITY_EPSILON: f64 = 1e-8;
const PLANE_DOT_EPSILON: f64 = 1e-9;
const PLANE_DISTANCE_EPSILON: f64 = 1e-5;
const UV_ERROR_WARNING: f64 = 0.25;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Nif(String),
    Reconstruction(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "IO error: {error}"),
            Self::Nif(error) => write!(f, "NIF error: {error}"),
            Self::Reconstruction(error) => write!(f, "reconstruction error: {error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "These flags are the intentionally mirrored prototype CLI contract."
)]
pub struct Options {
    pub inputs: Vec<PathBuf>,
    pub recursive: bool,
    pub output_dir: PathBuf,
    pub shell_thickness: f64,
    pub fallback_thickness: f64,
    pub fallback: String,
    pub skip_material: String,
    pub texture_roots: Vec<PathBuf>,
    pub texture_size: u32,
    pub flip_v: bool,
    pub include_collision: bool,
    pub overwrite: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub validate: bool,
    pub max_brushes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct P3 {
    x: f64,
    y: f64,
    z: f64,
}

impl P3 {
    const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
    fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }
    fn unit(self) -> Result<Self, Error> {
        let length = self.norm();
        if length <= 1e-12 {
            return Err(Error::Reconstruction("zero-length vector".into()));
        }
        Ok(self / length)
    }
    fn distance(self, other: Self) -> f64 {
        (self - other).norm()
    }
}

impl std::ops::Add for P3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}
impl std::ops::Sub for P3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}
impl std::ops::Mul<f64> for P3 {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}
impl std::ops::Div<f64> for P3 {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        self * (1.0 / rhs)
    }
}
impl std::ops::Neg for P3 {
    type Output = Self;
    fn neg(self) -> Self {
        self * -1.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Q {
    x: i64,
    y: i64,
}

impl Q {
    fn as_coord(self) -> Coord<f64> {
        Coord {
            x: self.x as f64 * WELD_EPSILON,
            y: self.y as f64 * WELD_EPSILON,
        }
    }
}

fn qkey(p: Coord<f64>) -> Q {
    Q {
        x: (p.x / WELD_EPSILON).round() as i64,
        y: (p.y / WELD_EPSILON).round() as i64,
    }
}
fn p2(x: f64, y: f64) -> Coord<f64> {
    Coord { x, y }
}

#[derive(Clone, Debug)]
struct Projection {
    u: P3,
    u_shift: f64,
    u_scale: f64,
    v: P3,
    v_shift: f64,
    v_scale: f64,
    max_error: f64,
}

#[derive(Clone, Debug)]
struct VisualMesh {
    block: usize,
    name: String,
    vertices: Vec<P3>,
    uvs: Option<Vec<[f64; 2]>>,
    triangles: Vec<[usize; 3]>,
    material: String,
    source_texture: Option<String>,
    texture_size: (u32, u32),
}

#[derive(Clone, Debug)]
struct Segment {
    a: Coord<f64>,
    b: Coord<f64>,
    material: String,
    projection: Option<Projection>,
    shape: usize,
}

#[derive(Clone, Debug)]
struct Cap {
    polygon: Polygon<f64>,
    material: String,
    projection: Option<Projection>,
}

#[derive(Clone, Debug)]
struct Sweep {
    direction: P3,
    u: P3,
    v: P3,
    layers: Vec<f64>,
    t_min: f64,
    t_max: f64,
    similarity: f64,
    score: f64,
}

#[derive(Clone, Debug)]
struct Brush {
    faces: Vec<String>,
    kind: String,
    shapes: Vec<usize>,
}

#[derive(Default, Debug)]
struct Reconstruction {
    brushes: Vec<Brush>,
    used_shapes: HashSet<usize>,
    recognizers: Vec<RecognizerReport>,
    warnings: Vec<String>,
    uv_max_error: f64,
}

#[derive(Clone, Debug, Serialize)]
struct RecognizerReport {
    #[serde(rename = "type")]
    kind: String,
    shapes: Vec<usize>,
    #[serde(flatten)]
    details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
struct ShapeReport {
    block: usize,
    name: String,
    vertices: usize,
    triangles: usize,
    material: String,
    source_texture: Option<String>,
    texture_size: [u32; 2],
}

#[derive(Clone, Debug, Serialize)]
struct Report {
    source: String,
    output: Option<String>,
    visual_shapes: Vec<ShapeReport>,
    brushes: usize,
    brush_faces: usize,
    used_shapes: Vec<usize>,
    recognizers: Vec<RecognizerReport>,
    max_triangle_uv_fit_error_texels: f64,
    warnings: Vec<String>,
}

type JobOutput = (usize, usize, Report);
type JobResult = (PathBuf, Result<Option<JobOutput>, Error>);

fn unit(v: P3) -> Result<P3, Error> {
    v.unit()
}

fn canonical_direction(v: P3) -> Result<P3, Error> {
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

fn stable_basis(direction: P3) -> Result<(P3, P3), Error> {
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

fn fmt(value: f64) -> String {
    if (value - value.round()).abs() <= 1e-6 {
        return format!("{}", value.round() as i64);
    }
    format!("{value:.8}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn fmt_point(point: P3) -> String {
    format!("{} {} {}", fmt(point.x), fmt(point.y), fmt(point.z))
}

fn poly(coords: Vec<Coord<f64>>) -> Polygon<f64> {
    let mut ring = coords;
    if ring.first() != ring.last()
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    Polygon::new(LineString::from(ring), Vec::new())
}

fn polygon_area(polygon: &Polygon<f64>) -> f64 {
    polygon.unsigned_area()
}

fn ring_points(polygon: &Polygon<f64>) -> Vec<Coord<f64>> {
    polygon
        .exterior()
        .0
        .iter()
        .copied()
        .take_while(|_| true)
        .collect::<Vec<_>>()
}

fn remove_collinear(mut points: Vec<Coord<f64>>) -> Vec<Coord<f64>> {
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

fn polygon_is_convex(polygon: &Polygon<f64>) -> bool {
    if !polygon.interiors().is_empty() {
        return false;
    }
    let area = polygon_area(polygon);
    (area - polygon.convex_hull().unsigned_area()).abs() <= 1e-7 * area.max(1.0)
}

fn representative(polygon: &Polygon<f64>) -> Point<f64> {
    polygon
        .centroid()
        .filter(|point| polygon.covers(point))
        .unwrap_or_else(|| Point::from(polygon.exterior().0[0]))
}

fn relative_error(lhs: f64, rhs: f64) -> f64 {
    (lhs - rhs).abs() / lhs.abs().max(rhs.abs()).max(1.0)
}

fn ring_length(ring: &LineString<f64>) -> f64 {
    ring.0
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum()
}

fn boundary_hausdorff(lhs: &Polygon<f64>, rhs: &Polygon<f64>) -> f64 {
    lhs.exterior().hausdorff_distance(rhs.exterior())
}

fn cluster_values(mut values: Vec<f64>, epsilon: f64) -> Vec<f64> {
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

fn nearest_layer(value: f64, layers: &[f64]) -> usize {
    layers
        .iter()
        .enumerate()
        .min_by(|a, b| (value - *a.1).abs().total_cmp(&(value - *b.1).abs()))
        .map_or(0, |(index, _)| index)
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

fn candidate_directions(mesh: &VisualMesh) -> Vec<P3> {
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

fn analyze_sweep(mesh: &VisualMesh, direction: P3) -> Option<Sweep> {
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

fn detect_sweep(mesh: &VisualMesh) -> Option<Sweep> {
    candidate_directions(mesh)
        .into_iter()
        .filter_map(|direction| analyze_sweep(mesh, direction))
        .max_by(|a, b| a.score.total_cmp(&b.score))
}

fn compatible_sweep(lhs: &Sweep, rhs: &Sweep) -> bool {
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

fn plane_from_points(points: [P3; 3]) -> Result<(P3, f64), Error> {
    let normal = unit((points[1] - points[0]).cross(points[2] - points[0]))?;
    Ok((normal, normal.dot(points[0])))
}

fn fit_projection(
    points: [P3; 3],
    texcoords: [[f64; 2]; 3],
    texture_size: (u32, u32),
    flip_v: bool,
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
            (if flip_v {
                1.0 - texcoords[0][1]
            } else {
                texcoords[0][1]
            }) * height,
            (if flip_v {
                1.0 - texcoords[1][1]
            } else {
                texcoords[1][1]
            }) * height,
            (if flip_v {
                1.0 - texcoords[2][1]
            } else {
                texcoords[2][1]
            }) * height,
        );
    let lift = |fit: nalgebra::Vector3<f64>| {
        let gradient = tangent_u * fit[0] + tangent_v * fit[1];
        let magnitude = gradient.norm();
        if magnitude <= 1e-12 {
            (tangent_u, fit[2], 1.0)
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
        let expected_t = (if flip_v { 1.0 - uv[1] } else { uv[1] }) * height;
        max_error = max_error
            .max((actual_s - expected_s).abs())
            .max((actual_t - expected_t).abs());
    }
    Some(Projection {
        u,
        u_shift,
        u_scale,
        v,
        v_shift,
        v_scale,
        max_error,
    })
}

fn triangle_projection(mesh: &VisualMesh, index: usize, flip_v: bool) -> Option<Projection> {
    let uvs = mesh.uvs.as_ref()?;
    let triangle = mesh.triangles[index];
    fit_projection(
        [
            mesh.vertices[triangle[0]],
            mesh.vertices[triangle[1]],
            mesh.vertices[triangle[2]],
        ],
        [uvs[triangle[0]], uvs[triangle[1]], uvs[triangle[2]]],
        mesh.texture_size,
        flip_v,
    )
}

fn project(point: P3, sweep: &Sweep) -> Coord<f64> {
    Coord {
        x: point.dot(sweep.u),
        y: point.dot(sweep.v),
    }
}
fn unproject(point: Coord<f64>, t: f64, sweep: &Sweep) -> P3 {
    sweep.u * point.x + sweep.v * point.y + sweep.direction * t
}

fn collect_caps(meshes: &[VisualMesh], sweep: &Sweep, flip_v: bool) -> (Vec<Cap>, Vec<Cap>) {
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
                projection: triangle_projection(mesh, triangle_index, flip_v),
            });
        }
    }
    (low, high)
}

fn collect_segments(
    meshes: &[VisualMesh],
    sweep: &Sweep,
    flip_v: bool,
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
            let projection = triangle_projection(mesh, triangle_index, flip_v);
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

fn profile_cycles(segments: &[Segment]) -> Option<Vec<Polygon<f64>>> {
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

fn profile_area_signature(cycles: &[Polygon<f64>]) -> Vec<f64> {
    let mut areas: Vec<_> = cycles.iter().map(polygon_area).collect();
    areas.sort_by(f64::total_cmp);
    areas
}

type ProfileValidation = (Vec<Segment>, Vec<Polygon<f64>>, f64, f64, f64);

fn validate_profile(
    meshes: &[VisualMesh],
    sweep: &Sweep,
    flip_v: bool,
) -> Option<ProfileValidation> {
    let mut profiles = Vec::new();
    for layer in 0..sweep.layers.len() {
        let segments = collect_segments(meshes, sweep, flip_v, Some(layer));
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

fn projected_layer_sets(meshes: &[VisualMesh], sweep: &Sweep) -> Vec<HashSet<Q>> {
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

fn exact_layer_invariance(meshes: &[VisualMesh], sweep: &Sweep) -> bool {
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

fn triangulation_decomposition(target: &Polygon<f64>) -> Vec<Polygon<f64>> {
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

fn decomposition_score(target: &Polygon<f64>, pieces: &[Polygon<f64>]) -> f64 {
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

fn rounded_level(value: f64) -> f64 {
    (value * 1e8).round() / 1e8
}

fn slice_decomposition(target: &Polygon<f64>, axis: usize) -> Vec<Polygon<f64>> {
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

fn greedy_convex_merge(polygons: Vec<Polygon<f64>>) -> Vec<Polygon<f64>> {
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

fn decompose(target: &Polygon<f64>) -> Result<Vec<Polygon<f64>>, Error> {
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

fn tb_triplet(points: &[P3], desired_normal: P3) -> Result<(P3, P3, P3), Error> {
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

fn generic_projection(normal: P3) -> Projection {
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

fn emit_face(
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

fn source_for_edge(sources: &[Segment], a: Coord<f64>, b: Coord<f64>) -> Option<&Segment> {
    let midpoint = p2(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
    sources.iter().find(|source| {
        point_segment_distance(source.a, source.b, a) <= 2e-3
            && point_segment_distance(source.a, source.b, b) <= 2e-3
            && point_segment_distance(source.a, source.b, midpoint) <= 2e-3
    })
}

fn point_segment_distance(a: Coord<f64>, b: Coord<f64>, point: Coord<f64>) -> f64 {
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

fn source_cap<'a>(caps: &'a [Cap], point: &Point<f64>) -> Option<&'a Cap> {
    caps.iter()
        .find(|cap| cap.polygon.buffer(1e-6).covers(point))
}

struct ExtrusionSources<'a> {
    side: &'a [Segment],
    low: &'a [Cap],
    high: &'a [Cap],
}

fn extrude_pieces(
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
        });
    }
    Ok(brushes)
}

fn union_polygons(polygons: &[Polygon<f64>]) -> MultiPolygon<f64> {
    unary_union(polygons)
}

fn exact_extrusion(
    meshes: &[VisualMesh],
    sweep: &Sweep,
    flip_v: bool,
    skip: &str,
) -> Option<(Vec<Brush>, RecognizerReport)> {
    if !exact_layer_invariance(meshes, sweep) {
        return None;
    }
    let (low_caps, high_caps) = collect_caps(meshes, sweep, flip_v);
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
    let side_sources = collect_segments(meshes, sweep, flip_v, None);
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

fn swept_shell(
    meshes: &[VisualMesh],
    sweep: &Sweep,
    thickness: f64,
    flip_v: bool,
    skip: &str,
) -> Option<(Vec<Brush>, RecognizerReport)> {
    let (low_caps, high_caps) = collect_caps(meshes, sweep, flip_v);
    let cap_area: f64 = low_caps
        .iter()
        .chain(&high_caps)
        .map(|cap| polygon_area(&cap.polygon))
        .sum();
    if cap_area > 1e-5 {
        return None;
    }
    let (segments, cycles, max_hausdorff, max_length, max_area) =
        validate_profile(meshes, sweep, flip_v)?;
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

fn group_open_shell_shapes(
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
        let segments = collect_segments(group, seed_sweep, true, Some(0));
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

fn planar_fallback(
    mesh: &VisualMesh,
    thickness: f64,
    flip_v: bool,
    skip: &str,
) -> Result<(Vec<Brush>, RecognizerReport), Error> {
    let mut groups: Vec<(P3, f64, Vec<usize>)> = Vec::new();
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
        if let Some((_, _, indices)) =
            groups
                .iter_mut()
                .find(|(candidate, candidate_distance, _)| {
                    candidate.dot(normal).abs() >= 0.99999
                        && (*candidate_distance - distance).abs() <= 1e-3
                })
        {
            indices.push(triangle_index);
        } else {
            groups.push((normal, distance, vec![triangle_index]));
        }
    }
    let mut brushes = Vec::new();
    for (_, _, triangle_indices) in &groups {
        brushes.extend(fallback_group(
            mesh,
            triangle_indices,
            thickness,
            flip_v,
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
    thickness: f64,
    flip_v: bool,
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
    let mut projections = Vec::new();
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
            projections.push((
                *triangle_index,
                triangle_projection(mesh, *triangle_index, flip_v),
            ));
        }
    }
    if polygons.is_empty() {
        return Ok(Vec::new());
    }
    let context = FallbackContext {
        mesh,
        projections: &projections,
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

struct FallbackContext<'a> {
    mesh: &'a VisualMesh,
    projections: &'a [(usize, Option<Projection>)],
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
    let representative_point = representative(&poly(coordinates.clone()));
    let source_projection = context
        .projections
        .iter()
        .find(|(index, _)| {
            let triangle = context.mesh.triangles[*index];
            let source = poly(
                [
                    context.mesh.vertices[triangle[0]],
                    context.mesh.vertices[triangle[1]],
                    context.mesh.vertices[triangle[2]],
                ]
                .into_iter()
                .map(|point| {
                    p2(
                        (point - context.origin).dot(context.tangent_u),
                        (point - context.origin).dot(context.tangent_v),
                    )
                })
                .collect(),
            );
            source.buffer(1e-6).covers(&representative_point)
        })
        .and_then(|(_, projection)| projection.as_ref());
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
            }),
    )
}

fn average_points(points: Vec<P3>) -> P3 {
    let count = points.len().max(1) as f64;
    let sum = points
        .into_iter()
        .fold(P3::default(), |sum, point| sum + point);
    sum / count
}

fn emitted_plane(line: &str) -> Result<(P3, f64), Error> {
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

fn solve_planes(first: (P3, f64), second: (P3, f64), third: (P3, f64)) -> Option<P3> {
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

fn validate_brush(brush: &Brush) -> Result<usize, Error> {
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

fn reject_duplicate_planes(planes: &[(P3, f64)]) -> Result<(), Error> {
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

fn brush_vertices(planes: &[(P3, f64)], brush: &Brush) -> Result<Vec<P3>, Error> {
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

fn validate_brush_volume(planes: &[(P3, f64)], vertices: &[P3]) -> Result<(), Error> {
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

fn reconstruct(meshes: &[VisualMesh], options: &Options) -> Result<Reconstruction, Error> {
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
    result.uv_max_error = max_uv_error(meshes, options.flip_v);
    Ok(result)
}

fn process_seed(
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
    if let Some((brushes, report)) = exact_extrusion(
        std::slice::from_ref(seed),
        sweep,
        options.flip_v,
        &options.skip_material,
    ) {
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
        options.flip_v,
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
    if let Some((brushes, report)) = exact_extrusion(
        std::slice::from_ref(seed),
        sweep,
        options.flip_v,
        &options.skip_material,
    ) {
        result.brushes.extend(brushes);
        result.recognizers.push(report);
        result.used_shapes.insert(seed.block);
        remaining.remove(&seed.block);
    }
}

fn apply_fallback(
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
        match planar_fallback(
            &mesh,
            options.fallback_thickness,
            options.flip_v,
            &options.skip_material,
        ) {
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

fn report_unsupported(remaining: &HashMap<usize, VisualMesh>, result: &mut Reconstruction) {
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

fn validate_reconstruction(result: &mut Reconstruction) -> Result<(), Error> {
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

fn max_uv_error(meshes: &[VisualMesh], flip_v: bool) -> f64 {
    meshes
        .iter()
        .filter(|mesh| mesh.uvs.is_some())
        .flat_map(|mesh| (0..mesh.triangles.len()).map(move |index| (mesh, index)))
        .filter_map(|(mesh, index)| triangle_projection(mesh, index, flip_v))
        .map(|projection| projection.max_error)
        .fold(0.0, f64::max)
}

#[derive(Debug)]
struct TextureResolver {
    roots: Vec<PathBuf>,
    fallback: (u32, u32),
    index: OnceLock<HashMap<String, PathBuf>>,
}

impl TextureResolver {
    fn new(roots: &[PathBuf], fallback: u32) -> Self {
        Self {
            roots: roots.to_vec(),
            fallback: (fallback, fallback),
            index: OnceLock::new(),
        }
    }

    fn build_index(&self) -> HashMap<String, PathBuf> {
        fn visit(root: &Path, current: &Path, index: &mut HashMap<String, PathBuf>) {
            let Ok(entries) = fs::read_dir(current) else {
                return;
            };
            let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
            entries.sort_by_key(std::fs::DirEntry::path);
            for entry in entries {
                let path = entry.path();
                if path.is_dir() {
                    visit(root, &path, index);
                    continue;
                }
                let Some(extension) = path.extension().and_then(|extension| extension.to_str())
                else {
                    continue;
                };
                if !matches!(extension.to_ascii_lowercase().as_str(), "tga" | "dds") {
                    continue;
                }
                if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                    index
                        .entry(name.to_ascii_lowercase())
                        .or_insert_with(|| path.clone());
                }
                if let Ok(relative) = path.strip_prefix(root) {
                    index
                        .entry(
                            relative
                                .to_string_lossy()
                                .replace('\\', "/")
                                .to_ascii_lowercase(),
                        )
                        .or_insert(path);
                }
            }
        }
        let mut index = HashMap::new();
        for root in &self.roots {
            let root = root.canonicalize().unwrap_or_else(|_| root.clone());
            if root.exists() {
                visit(&root, &root, &mut index);
            }
        }
        index
    }

    fn dimensions(&self, source_texture: Option<&str>) -> (u32, u32) {
        let Some(source_texture) = source_texture else {
            return self.fallback;
        };
        if self.roots.is_empty() {
            return self.fallback;
        }
        let index = self.index.get_or_init(|| self.build_index());
        let key = source_texture.replace('\\', "/").to_ascii_lowercase();
        let Some(path) = index.get(&key).or_else(|| {
            Path::new(&key)
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| index.get(name))
        }) else {
            return self.fallback;
        };
        let Ok(bytes) = fs::read(path) else {
            return self.fallback;
        };
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("dds") if bytes.len() >= 20 && &bytes[..4] == b"DDS " => (
                u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
                u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            ),
            Some("tga") if bytes.len() >= 18 => (
                u32::from(u16::from_le_bytes(bytes[12..14].try_into().unwrap())),
                u32::from(u16::from_le_bytes(bytes[14..16].try_into().unwrap())),
            ),
            _ => self.fallback,
        }
    }
}

fn material_name(source_texture: Option<&str>) -> String {
    let Some(texture) = source_texture else {
        return "nif2map/missing".into();
    };
    let mut material = texture.replace('\\', "/");
    if let Some(dot) = material.rfind('.') {
        material.truncate(dot);
    }
    if material.to_ascii_lowercase().starts_with("textures/") {
        material.drain(..9);
    }
    if material.is_empty() {
        "nif2map/missing".into()
    } else {
        material
    }
}

fn collision_descendants(stream: &NiStream) -> HashSet<NiKey> {
    let mut descendants = HashSet::new();
    let roots: Vec<_> = stream
        .objects_of_type_with_link::<RootCollisionNode>()
        .map(|(key, root)| (key.key, root.base.children.clone()))
        .collect();
    let mut stack = Vec::new();
    for (key, children) in roots {
        descendants.insert(key);
        stack.extend(children);
    }
    while let Some(link) = stack.pop() {
        if !descendants.insert(link.key) {
            continue;
        }
        if let Some(node) = stream.get_as::<_, NiNode>(link) {
            stack.extend(node.children.iter().copied());
        }
    }
    descendants
}

fn reject_legacy_bounds(stream: &NiStream, path: &Path) -> Result<(), Error> {
    for (_, node) in stream.objects_of_type_with_link::<NiNode>() {
        if node.base.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: NiNode uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    for (_, root) in stream.objects_of_type_with_link::<RootCollisionNode>() {
        if root.base.base.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: RootCollisionNode uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    for (_, shape) in stream.objects_of_type_with_link::<NiTriShape>() {
        if shape.base.base.base.bounding_volume.is_some() {
            return Err(Error::Nif(format!(
                "{}: NiTriShape uses an unsupported legacy bounding volume",
                path.display()
            )));
        }
    }
    Ok(())
}

fn shape_texture(stream: &NiStream, shape: &NiTriShape) -> Option<String> {
    let properties = &shape.base.base.base.properties;
    for property in properties {
        let Some(texturing) = stream.get_as::<_, NiTexturingProperty>(*property) else {
            continue;
        };
        for map in &texturing.texture_maps {
            let Some(TextureMap::Map(map)) = map else {
                continue;
            };
            let Some(source) = stream.get(map.texture) else {
                continue;
            };
            if let TextureSource::External(path) = &source.source {
                return Some(path.clone());
            }
        }
    }
    None
}

fn nif_meshes(
    path: &Path,
    resolver: &TextureResolver,
    include_collision: bool,
) -> Result<Vec<VisualMesh>, Error> {
    let stream = NiStream::from_path(path)
        .map_err(|error| Error::Nif(format!("{}: {error}", path.display())))?;
    reject_legacy_bounds(&stream, path)?;
    let ordinals: HashMap<NiKey, usize> = stream
        .objects
        .iter()
        .enumerate()
        .map(|(index, (key, _))| (key, index))
        .collect();
    let transforms = stream.world_transforms();
    let collision = collision_descendants(&stream);
    let mut meshes = Vec::new();
    for (link, shape) in stream.objects_of_type_with_link::<NiTriShape>() {
        if !include_collision && collision.contains(&link.key) {
            continue;
        }
        let Some(data) = stream.get_as::<_, NiTriShapeData>(shape.base.base.geometry_data) else {
            continue;
        };
        let Some(&block) = ordinals.get(&link.key) else {
            continue;
        };
        let transform = transforms
            .get(&link.key)
            .copied()
            .unwrap_or_else(|| shape.base.base.base.transform());
        let vertices: Vec<_> = data
            .base
            .base
            .vertices
            .iter()
            .map(|vertex| {
                let point = transform.transform_point3(*vertex);
                P3 {
                    x: f64::from(point.x),
                    y: f64::from(point.y),
                    z: f64::from(point.z),
                }
            })
            .collect();
        let uvs = data.base.base.uv_set(0).map(|uvs| {
            uvs.iter()
                .map(|uv| [f64::from(uv.x), f64::from(uv.y)])
                .collect()
        });
        let triangles = data
            .triangles
            .iter()
            .map(|triangle| {
                [
                    triangle[0] as usize,
                    triangle[1] as usize,
                    triangle[2] as usize,
                ]
            })
            .collect();
        let source_texture = shape_texture(&stream, shape);
        meshes.push(VisualMesh {
            block,
            name: shape.base.base.base.base.name.clone(),
            vertices,
            uvs,
            triangles,
            material: material_name(source_texture.as_deref()),
            texture_size: resolver.dimensions(source_texture.as_deref()),
            source_texture,
        });
    }
    Ok(meshes)
}

fn gather_inputs(paths: &[PathBuf], recursive: bool) -> Vec<PathBuf> {
    fn visit(path: &Path, recursive: bool, collected: &mut Vec<PathBuf>) {
        if path.is_file() {
            if path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("nif"))
                && let Ok(path) = path.canonicalize()
            {
                collected.push(path);
            }
            return;
        }
        if !path.is_dir() {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for entry in entries {
            if entry.is_file() || recursive {
                visit(&entry, recursive, collected);
            }
        }
    }
    let mut collected = Vec::new();
    for path in paths {
        visit(path, recursive, &mut collected);
    }
    collected.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    collected.dedup();
    collected
}

fn sha1_suffix(path: &Path) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(4)
        .fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02x}").unwrap();
            output
        })
}

fn map_text(source: &Path, result: &Reconstruction) -> String {
    let mut lines = vec![
        format!(
            "// Reverse-compiled from {}",
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown.nif")
        ),
        "// nif2map.py experimental visual-geometry importer".to_string(),
        "// Artificial closure/partition faces use skip material.".into(),
        "{".into(),
        "\"classname\" \"worldspawn\"".into(),
        "\"mapversion\" \"220\"".into(),
        format!(
            "\"message\" \"nif2map: {}\"",
            source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("nif")
        ),
    ];
    for (index, brush) in result.brushes.iter().enumerate() {
        let shapes = brush
            .shapes
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        lines.push(format!(
            "// brush_{index:04} kind={} source_shapes={shapes}",
            brush.kind
        ));
        lines.push("{".into());
        lines.extend(brush.faces.iter().cloned());
        lines.push("}".into());
    }
    lines.push("}".into());
    lines.join("\n") + "\n"
}

fn report(
    source: &Path,
    output: Option<&Path>,
    meshes: &[VisualMesh],
    result: &Reconstruction,
) -> Report {
    Report {
        source: source.to_string_lossy().into_owned(),
        output: output.map(|path| path.to_string_lossy().into_owned()),
        visual_shapes: meshes
            .iter()
            .map(|mesh| ShapeReport {
                block: mesh.block,
                name: mesh.name.clone(),
                vertices: mesh.vertices.len(),
                triangles: mesh.triangles.len(),
                material: mesh.material.clone(),
                source_texture: mesh.source_texture.clone(),
                texture_size: [mesh.texture_size.0, mesh.texture_size.1],
            })
            .collect(),
        brushes: result.brushes.len(),
        brush_faces: result.brushes.iter().map(|brush| brush.faces.len()).sum(),
        used_shapes: {
            let mut shapes: Vec<_> = result.used_shapes.iter().copied().collect();
            shapes.sort_unstable();
            shapes
        },
        recognizers: result.recognizers.clone(),
        max_triangle_uv_fit_error_texels: result.uv_max_error,
        warnings: result.warnings.clone(),
    }
}

fn process_one(
    source: &Path,
    output: &Path,
    report_path: &Path,
    options: &Options,
    resolver: &TextureResolver,
) -> Result<(usize, usize, Report), Error> {
    let nif = nif_meshes(source, resolver, options.include_collision)?;
    if nif.is_empty() {
        return Err(Error::Reconstruction(
            "no visual NiTriShape meshes found".into(),
        ));
    }
    let result = reconstruct(&nif, options)?;
    if result.brushes.is_empty() {
        return Err(Error::Reconstruction("no brushes reconstructed".into()));
    }
    let report = report(
        source,
        if options.dry_run { None } else { Some(output) },
        &nif,
        &result,
    );
    if !options.dry_run {
        fs::write(output, map_text(source, &result))?;
        let json = serde_json::to_vec_pretty(&report)
            .map_err(|error| Error::Io(io::Error::other(error)))?;
        fs::write(report_path, [json.as_slice(), b"\n"].concat())?;
    }
    Ok((nif.len(), result.brushes.len(), report))
}

/// Run the native NIF reverse compiler. Independent files are analyzed in
/// parallel, while reporting and output order remain deterministic.
/// # Errors
///
/// Returns an error when inputs cannot be enumerated, a NIF cannot be parsed,
/// reconstruction fails, or an output cannot be written.
pub fn run(options: &Options) -> io::Result<()> {
    validate_options(options)?;
    let inputs = gather_inputs(&options.inputs, options.recursive);
    if inputs.is_empty() {
        eprintln!("nif2map: no .nif inputs found");
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no .nif inputs found",
        ));
    }
    fs::create_dir_all(&options.output_dir)?;
    let resolver = Arc::new(TextureResolver::new(
        &options.texture_roots,
        options.texture_size,
    ));
    let jobs = build_jobs(options, inputs);
    let input_count = jobs.len();
    let results = process_jobs(options, &resolver, &jobs);
    report_results(options, results, input_count)
}

fn validate_options(options: &Options) -> io::Result<()> {
    if options.shell_thickness <= 0.0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--shell-thickness must be > 0",
        ));
    }
    if options.fallback_thickness <= 0.0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--fallback-thickness must be > 0",
        ));
    }
    if options.texture_size == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--texture-size must be > 0",
        ));
    }
    if options.fallback != "planar-prisms" && options.fallback != "skip" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid fallback mode {:?}", options.fallback),
        ));
    }
    Ok(())
}

fn build_jobs(options: &Options, inputs: Vec<PathBuf>) -> Vec<(PathBuf, PathBuf, PathBuf)> {
    let mut stem_counts = HashMap::<String, usize>::new();
    for input in &inputs {
        *stem_counts
            .entry(
                input
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            )
            .or_default() += 1;
    }
    inputs
        .into_iter()
        .map(|source| {
            let stem = source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("nif")
                .to_string();
            let output_stem = if stem_counts
                .get(&stem.to_ascii_lowercase())
                .copied()
                .unwrap_or(0)
                > 1
            {
                format!("{stem}__{}", sha1_suffix(&source))
            } else {
                stem
            };
            let output = options.output_dir.join(format!("{output_stem}.map"));
            let report = options
                .output_dir
                .join(format!("{output_stem}.nif2map.json"));
            (source, output, report)
        })
        .collect()
}

fn process_jobs(
    options: &Options,
    resolver: &Arc<TextureResolver>,
    jobs: &[(PathBuf, PathBuf, PathBuf)],
) -> Vec<JobResult> {
    jobs.par_iter()
        .map(|(source, output, report)| {
            if !options.dry_run && !options.overwrite && (output.exists() || report.exists()) {
                return (source.clone(), Ok(None));
            }
            (
                source.clone(),
                process_one(source, output, report, options, resolver).map(Some),
            )
        })
        .collect()
}

fn report_results(
    options: &Options,
    results: Vec<JobResult>,
    input_count: usize,
) -> io::Result<()> {
    let mut succeeded = 0;
    let mut failed = 0;
    let mut skipped = 0;
    for (source, result) in results {
        match result {
            Ok(None) => {
                println!(
                    "SKIP  {} -> output exists (use --overwrite)",
                    source.display()
                );
                skipped += 1;
            }
            Ok(Some((shape_count, brush_count, report))) => {
                println!(
                    "OK    {}: {shape_count} visual shape(s) -> {brush_count} brush(es){}",
                    source
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown"),
                    if report.warnings.is_empty() {
                        String::new()
                    } else {
                        format!(", {} warning(s)", report.warnings.len())
                    }
                );
                if options.verbose {
                    for recognizer in &report.recognizers {
                        println!(
                            "      {}",
                            serde_json::to_string(recognizer).unwrap_or_default()
                        );
                    }
                    for warning in &report.warnings {
                        println!("      WARN {warning}");
                    }
                    if report.max_triangle_uv_fit_error_texels > UV_ERROR_WARNING {
                        println!(
                            "      UV diagnostic: max per-triangle affine fit error {:.6} texels",
                            report.max_triangle_uv_fit_error_texels
                        );
                    }
                }
                succeeded += 1;
            }
            Err(error) => {
                eprintln!("FAIL  {}: {error}", source.display());
                failed += 1;
            }
        }
    }
    println!(
        "\n{succeeded} succeeded, {failed} failed, {skipped} skipped ({input_count} input(s))"
    );
    if failed == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{failed} NIF conversion(s) failed"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tes3::nif::{NiNode, NiStream};

    fn mesh(vertices: Vec<P3>, triangles: Vec<[usize; 3]>) -> VisualMesh {
        VisualMesh {
            block: 7,
            name: "fixture".into(),
            vertices,
            uvs: None,
            triangles,
            material: "fixture/mat".into(),
            source_texture: None,
            texture_size: (256, 256),
        }
    }

    fn options(fallback: &str) -> Options {
        Options {
            inputs: Vec::new(),
            recursive: false,
            output_dir: PathBuf::new(),
            shell_thickness: 16.0,
            fallback_thickness: 2.0,
            fallback: fallback.into(),
            skip_material: "skip".into(),
            texture_roots: Vec::new(),
            texture_size: 256,
            flip_v: true,
            include_collision: false,
            overwrite: false,
            dry_run: false,
            verbose: false,
            validate: true,
            max_brushes: 20_000,
        }
    }

    #[test]
    fn exact_prism_uses_structural_recognizer() {
        let vertices = vec![
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 4.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 4.0,
                z: 3.0,
            },
            P3 {
                x: 0.0,
                y: 0.0,
                z: 3.0,
            },
            P3 {
                x: 10.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 10.0,
                y: 4.0,
                z: 0.0,
            },
            P3 {
                x: 10.0,
                y: 4.0,
                z: 3.0,
            },
            P3 {
                x: 10.0,
                y: 0.0,
                z: 3.0,
            },
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        let result = reconstruct(&[mesh(vertices, triangles)], &options("skip"))
            .expect("prism should reconstruct");
        assert_eq!(result.brushes.len(), 1);
        assert_eq!(result.recognizers[0].kind, "exact-extrusion");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn planar_fallback_emits_a_convex_prism() {
        let vertices = vec![
            P3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 4.0,
                y: 0.0,
                z: 0.0,
            },
            P3 {
                x: 0.0,
                y: 3.0,
                z: 0.0,
            },
        ];
        let result = reconstruct(
            &[mesh(vertices, vec![[0, 1, 2]])],
            &options("planar-prisms"),
        )
        .expect("triangle should fallback");
        assert_eq!(result.brushes.len(), 1);
        assert_eq!(result.recognizers[0].kind, "planar-prism-fallback");
        assert_eq!(validate_brush(&result.brushes[0]).unwrap(), 6);
    }

    #[test]
    fn nif_adapter_preserves_visual_shape_and_transform() {
        let mut stream = NiStream::default();
        let mut data = NiTriShapeData::default();
        data.base.base.vertices = vec![
            tes3::nif::glam::vec3(0.0, 0.0, 0.0),
            tes3::nif::glam::vec3(1.0, 0.0, 0.0),
            tes3::nif::glam::vec3(0.0, 1.0, 0.0),
        ];
        data.triangles = vec![[0, 1, 2]];
        let data_link = stream.insert(data);
        let mut shape = NiTriShape::default();
        shape.base.base.geometry_data = data_link.cast();
        shape.base.base.base.name = "fixture-shape".into();
        shape.base.base.base.translation = tes3::nif::glam::vec3(10.0, 20.0, 30.0);
        let shape_link = stream.insert(shape);
        let mut root = NiNode::default();
        root.children.push(shape_link.cast());
        let root_link = stream.insert(root);
        stream.roots.push(root_link.cast());
        let bytes = stream.save_bytes().expect("fixture NIF should serialize");
        let path = std::env::temp_dir().join(format!(
            "morrobroom-nif2map-{}-{}.nif",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, bytes).unwrap();
        let resolver = TextureResolver::new(&[], 256);
        let meshes = nif_meshes(&path, &resolver, false).expect("fixture NIF should parse");
        fs::remove_file(path).unwrap();
        assert_eq!(meshes.len(), 1);
        assert_eq!(meshes[0].name, "fixture-shape");
        assert_eq!(
            meshes[0].vertices[0],
            P3 {
                x: 10.0,
                y: 20.0,
                z: 30.0
            }
        );
    }

    #[test]
    fn map_header_matches_prototype_contract() {
        let result = Reconstruction {
            brushes: vec![Brush {
                faces: vec!["face".into()],
                kind: "test".into(),
                shapes: vec![1],
            }],
            ..Default::default()
        };
        let text = map_text(Path::new("fixture.nif"), &result);
        assert!(text.contains("// nif2map.py experimental visual-geometry importer"));
        assert!(text.contains("\"mapversion\" \"220\""));
    }

    #[test]
    fn texture_resolver_keeps_width_before_height() {
        let root = std::env::temp_dir().join(format!(
            "morrobroom-nif2map-texture-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.dds");
        let mut header = vec![0; 20];
        header[..4].copy_from_slice(b"DDS ");
        header[12..16].copy_from_slice(&32u32.to_le_bytes());
        header[16..20].copy_from_slice(&64u32.to_le_bytes());
        fs::write(&path, header).unwrap();
        let resolver = TextureResolver::new(std::slice::from_ref(&root), 256);
        assert_eq!(resolver.dimensions(Some("Textures/fixture.dds")), (64, 32));
        fs::remove_dir_all(root).unwrap();
    }
}
