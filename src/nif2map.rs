//! Native, batch-oriented replacement for the experimental `nif2map.py` tool.
//!
//! The importer keeps visual `NiTriShape` geometry authoritative, reconstructs
//! conservative convex brushes, and retains supported NIF state in a semantic
//! intermediate representation. Collision descendants are inspected for every
//! import and are emitted only when requested, in a separate authoring scope.
//! Geometry remains in compact `f64` structs and independent files are processed
//! in parallel; no Python or `NumPy` runtime is involved.
#![allow(
    clippy::cast_precision_loss,
    reason = "NIF geometry uses bounded mesh cardinalities and world coordinates."
)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "The prototype quantizes bounded geometry into reconstruction grid keys."
)]

use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
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
use imagesize::blob_size;
use nalgebra::Matrix3;
use rayon::prelude::*;
use serde::Serialize;
use tes3::nif::{
    NiAlphaProperty, NiBillboardNode, NiFloatData, NiFloatKey, NiKey, NiLink, NiMaterialProperty,
    NiNode, NiSortAdjustNode, NiStream, NiTexturingProperty, NiTriShape, NiTriShapeData,
    NiUVController, NiUVData, RootCollisionNode, TextureMap, TextureSource,
};
use vfstool_lib::VFS;

const WELD_EPSILON: f64 = 1e-3;
const LAYER_EPSILON: f64 = 1e-3;
const COLLINEAR_SINE_EPSILON: f64 = 1e-6;
const DIRECTION_DOT_EPSILON: f64 = 0.9995;
const GEOMETRY_EPSILON: f64 = 1e-5;
const FACE_TRIPLET_QUALITY_EPSILON: f64 = 1e-8;
const PLANE_DOT_EPSILON: f64 = 1e-9;
const PLANE_DISTANCE_EPSILON: f64 = 1e-5;
const UV_ERROR_WARNING: f64 = 0.25;
const UV_MERGE_TOLERANCE_TEXELS: f64 = 0.05;

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

fn projection_is_valid(projection: &Projection) -> bool {
    projection.u.x.is_finite()
        && projection.u.y.is_finite()
        && projection.u.z.is_finite()
        && projection.u_shift.is_finite()
        && projection.u_scale.is_finite()
        && projection.u_scale.abs() > f64::EPSILON
        && projection.v.x.is_finite()
        && projection.v.y.is_finite()
        && projection.v.z.is_finite()
        && projection.v_shift.is_finite()
        && projection.v_scale.is_finite()
        && projection.v_scale.abs() > f64::EPSILON
        && projection.max_error.is_finite()
}

fn projection_is_usable(projection: &Projection) -> bool {
    projection_is_valid(projection) && projection.max_error <= UV_MERGE_TOLERANCE_TEXELS
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
struct NifProperty {
    key: String,
    value: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
struct NifState {
    /// Only non-default authoring state is retained. The compiler owns the
    /// defaults; the map records overrides.
    properties: Vec<NifProperty>,
}

impl NifState {
    fn is_default(&self) -> bool {
        self.properties.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum ImportScope {
    #[default]
    Visual,
    Collision,
}

type ScopeId = usize;

#[derive(Clone, Debug, Serialize)]
struct SemanticScope {
    id: ScopeId,
    parent: Option<ScopeId>,
    name: String,
    kind: ImportScope,
    node_kind: String,
}

#[derive(Clone, Debug, Serialize)]
struct NifProvenance {
    av_flags: u16,
    has_controller: bool,
    has_extra_data: bool,
    has_skin_instance: bool,
}

#[derive(Clone, Debug, Serialize)]
struct TextureBinding {
    source: String,
    uv_set: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum TextureSizeSource {
    Resolved,
}

#[derive(Clone, Copy, Debug)]
struct TextureDimensions {
    size: (u32, u32),
    source: TextureSizeSource,
}

#[derive(Clone, Debug, Default)]
struct ImportedAsset {
    meshes: Vec<VisualMesh>,
    scopes: Vec<SemanticScope>,
    nodes: Vec<ImportedNode>,
    markers: Vec<ImportedMarker>,
    diagnostics: Vec<String>,
    timings: StageTimings,
}

#[derive(Clone, Copy, Debug, Default)]
struct StageTimings {
    parse_semantic: Duration,
    texture_resolution: Duration,
    sweep_analysis: Duration,
    structural_recognition: Duration,
    planar_fallback: Duration,
    brush_validation: Duration,
    uv_diagnostics: Duration,
    map_serialization: Duration,
    report_serialization: Duration,
    total: Duration,
}

impl StageTimings {
    fn add_assign(&mut self, other: Self) {
        self.parse_semantic += other.parse_semantic;
        self.texture_resolution += other.texture_resolution;
        self.sweep_analysis += other.sweep_analysis;
        self.structural_recognition += other.structural_recognition;
        self.planar_fallback += other.planar_fallback;
        self.brush_validation += other.brush_validation;
        self.uv_diagnostics += other.uv_diagnostics;
        self.map_serialization += other.map_serialization;
        self.report_serialization += other.report_serialization;
        self.total += other.total;
    }
}

#[derive(Clone, Debug, Serialize)]
struct ImportedNode {
    name: String,
    flags: u16,
    effects: usize,
    has_controller: bool,
    has_extra_data: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ImportedMarker {
    classname: String,
    name: String,
    origin: [f64; 3],
    scope: ScopeId,
    properties: Vec<NifProperty>,
}

#[derive(Clone, Debug)]
struct VisualMesh {
    block: usize,
    name: String,
    vertices: Vec<P3>,
    uvs: Option<Vec<[f64; 2]>>,
    triangles: Vec<[usize; 3]>,
    material: String,
    texture: Option<TextureBinding>,
    texture_size: Option<TextureDimensions>,
    scope: ScopeId,
    nif_state: NifState,
    provenance: NifProvenance,
    diagnostics: Vec<String>,
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

#[derive(Clone, Debug, Default)]
struct Brush {
    faces: Vec<String>,
    planes: Vec<(P3, f64)>,
    kind: String,
    shapes: Vec<usize>,
    scope: ScopeId,
    nif_state: NifState,
}

#[derive(Default, Debug)]
struct Reconstruction {
    brushes: Vec<Brush>,
    markers: Vec<ImportedMarker>,
    used_shapes: HashSet<usize>,
    recognizers: Vec<RecognizerReport>,
    warnings: Vec<String>,
    uv_max_error: f64,
    timings: StageTimings,
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
    texture: Option<TextureBinding>,
    texture_size: Option<[u32; 2]>,
    texture_size_source: Option<TextureSizeSource>,
    scope: ScopeId,
    scope_kind: ImportScope,
    nif_state: NifState,
    provenance: NifProvenance,
    diagnostics: Vec<String>,
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
    import_diagnostics: Vec<String>,
    nodes: Vec<ImportedNode>,
    scopes: Vec<SemanticScope>,
    markers: Vec<ImportedMarker>,
    #[serde(skip)]
    timings: StageTimings,
}

type JobOutput = (usize, usize, Report);
type JobResult = (PathBuf, Result<Option<JobOutput>, Error>);

mod geometry;
mod semantic;
mod texture;

use geometry::{
    CANONICAL_ALPHA, CANONICAL_ALPHA_TEST_THRESHOLD, STATE_FLOAT_EPSILON, canonical_float,
    reconstruct,
};
use semantic::import_scene;
use texture::TextureResolver;

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

fn emit_tb_group(lines: &mut Vec<String>, name: &str, id: usize, parent: Option<usize>) {
    lines.push("{".into());
    lines.push("\"classname\" \"func_group\"".into());
    lines.push("\"_tb_type\" \"_tb_group\"".into());
    lines.push(format!("\"_tb_name\" {name:?}"));
    lines.push(format!("\"_tb_id\" \"{id}\""));
    if let Some(parent) = parent {
        lines.push(format!("\"_tb_group\" \"{parent}\""));
    }
    lines.push("}".into());
}

fn emit_brush_entity(lines: &mut Vec<String>, parent: usize, state: &NifState, brushes: &[&Brush]) {
    lines.push("{".into());
    lines.push("\"classname\" \"nif_geometry\"".into());
    lines.push(format!("\"_tb_group\" \"{parent}\""));
    for property in &state.properties {
        lines.push(format!("{:?} {:?}", property.key, property.value));
    }
    for brush in brushes {
        lines.push("{".into());
        lines.extend(brush.faces.iter().cloned());
        lines.push("}".into());
    }
    lines.push("}".into());
}

fn emit_marker(lines: &mut Vec<String>, marker: &ImportedMarker, parent: usize) {
    lines.push("{".into());
    lines.push(format!("\"classname\" {:?}", marker.classname));
    lines.push(format!(
        "\"origin\" \"{} {} {}\"",
        marker.origin[0], marker.origin[1], marker.origin[2]
    ));
    lines.push(format!("\"_tb_group\" \"{parent}\""));
    for property in &marker.properties {
        lines.push(format!("{:?} {:?}", property.key, property.value));
    }
    lines.push("}".into());
}

fn brush_groups(result: &Reconstruction) -> Vec<(ScopeId, NifState, Vec<&Brush>)> {
    let mut groups: Vec<(ScopeId, NifState, Vec<&Brush>)> = Vec::new();
    for brush in &result.brushes {
        if let Some((_, _, brushes)) = groups
            .iter_mut()
            .find(|(scope, state, _)| *scope == brush.scope && *state == brush.nif_state)
        {
            brushes.push(brush);
        } else {
            groups.push((brush.scope, brush.nif_state.clone(), vec![brush]));
        }
    }
    groups
}

fn scope_kind(scopes: &[SemanticScope], id: ScopeId) -> ImportScope {
    scopes
        .get(id)
        .map_or(ImportScope::Visual, |scope| scope.kind)
}

fn map_text(
    source: &Path,
    result: &Reconstruction,
    scopes: &[SemanticScope],
    include_collision: bool,
) -> String {
    let asset_name = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Imported NIF");
    let mut lines = vec![
        "// Game: Morrowind".to_string(),
        "// Format: Quake2 (Valve)".to_string(),
        format!(
            "// Reverse-compiled from {}",
            source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown.nif")
        ),
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
        "}".into(),
    ];
    let groups = brush_groups(result);
    emit_tb_group(&mut lines, asset_name, 1, None);

    let mut scope_group_ids = HashMap::from([(0_usize, 1_usize)]);
    for (group_id, scope) in
        (2..).zip(scopes.iter().filter(|scope| {
            scope.id != 0 && (include_collision || scope.kind == ImportScope::Visual)
        }))
    {
        let parent = scope
            .parent
            .and_then(|parent| scope_group_ids.get(&parent).copied())
            .unwrap_or(1);
        let scope_name = if scope.name.is_empty() {
            scope.node_kind.as_str()
        } else {
            scope.name.as_str()
        };
        let group_name = match scope.kind {
            ImportScope::Visual => scope_name.to_owned(),
            ImportScope::Collision => format!("Collision: {scope_name}"),
        };
        emit_tb_group(&mut lines, &group_name, group_id, Some(parent));
        scope_group_ids.insert(scope.id, group_id);
    }
    for (scope, state, brushes) in groups {
        let parent = scope_group_ids.get(&scope).copied().unwrap_or(1);
        if scope == 0 && !state.is_default() {
            emit_brush_entity(&mut lines, parent, &state, &brushes);
            continue;
        }
        emit_brush_entity(&mut lines, parent, &state, &brushes);
    }

    for marker in &result.markers {
        if !include_collision && scope_kind(scopes, marker.scope) == ImportScope::Collision {
            continue;
        }
        let parent = scope_group_ids.get(&marker.scope).copied().unwrap_or(1);
        emit_marker(&mut lines, marker, parent);
    }
    lines.join("\n") + "\n"
}

fn report(
    source: &Path,
    output: Option<&Path>,
    meshes: &[VisualMesh],
    result: &Reconstruction,
    imported: &ImportedAsset,
    timings: StageTimings,
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
                texture: mesh.texture.clone(),
                texture_size: mesh
                    .texture_size
                    .map(|texture_size| [texture_size.size.0, texture_size.size.1]),
                texture_size_source: mesh.texture_size.map(|texture_size| texture_size.source),
                scope: mesh.scope,
                scope_kind: scope_kind(&imported.scopes, mesh.scope),
                nif_state: mesh.nif_state.clone(),
                provenance: mesh.provenance.clone(),
                diagnostics: mesh.diagnostics.clone(),
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
        import_diagnostics: imported.diagnostics.clone(),
        nodes: imported.nodes.clone(),
        scopes: imported.scopes.clone(),
        markers: imported.markers.clone(),
        timings,
    }
}

fn process_one(
    source: &Path,
    output: &Path,
    report_path: &Path,
    options: &Options,
    resolver: &TextureResolver,
) -> Result<(usize, usize, Report), Error> {
    let started = Instant::now();
    let imported = import_scene(source, resolver)?;
    let nif: Vec<_> = imported
        .meshes
        .iter()
        .filter(|mesh| {
            options.include_collision
                || scope_kind(&imported.scopes, mesh.scope) == ImportScope::Visual
        })
        .cloned()
        .collect();
    if nif.is_empty() {
        return Err(Error::Reconstruction(
            "no visual NiTriShape meshes found".into(),
        ));
    }
    let mut result = reconstruct(&nif, options)?;
    let mut timings = imported.timings;
    timings.add_assign(result.timings);
    result.markers.clone_from(&imported.markers);
    for mesh in &nif {
        for diagnostic in &mesh.diagnostics {
            result.warnings.push(format!(
                "shape {} {:?}: {diagnostic}",
                mesh.block, mesh.name
            ));
        }
    }
    result.warnings.extend(imported.diagnostics.iter().cloned());
    if result.brushes.is_empty() {
        return Err(Error::Reconstruction("no brushes reconstructed".into()));
    }
    let map_started = Instant::now();
    let map = map_text(source, &result, &imported.scopes, options.include_collision);
    timings.map_serialization += map_started.elapsed();
    let report = report(
        source,
        if options.dry_run { None } else { Some(output) },
        &nif,
        &result,
        &imported,
        timings,
    );
    if !options.dry_run {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, map)?;
        let report_started = Instant::now();
        let json = serde_json::to_vec_pretty(&report)
            .map_err(|error| Error::Io(io::Error::other(error)))?;
        timings.report_serialization += report_started.elapsed();
        fs::write(report_path, [json.as_slice(), b"\n"].concat())?;
    }
    let mut report = report;
    report.timings = timings;
    report.timings.total = started.elapsed();
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
    let resolver = Arc::new(TextureResolver::new(&options.texture_roots)?);
    let jobs = build_jobs(options, inputs);
    let input_count = jobs.len();
    let batch_started = Instant::now();
    let results = process_jobs(options, &resolver, &jobs);
    report_results(options, results, input_count, batch_started.elapsed())
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
    if options.texture_roots.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "at least one --texture-root is required",
        ));
    }
    for root in &options.texture_roots {
        if root.is_dir() {
            continue;
        }
        let is_archive = root.is_file()
            && root
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_ascii_lowercase().as_str(),
                        "bsa" | "ba2" | "zip"
                    )
                });
        if !is_archive {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "--texture-root {} must be an existing directory or a .bsa, .ba2, or .zip archive",
                    root.display()
                ),
            ));
        }
    }
    if options.fallback != "planar-prisms" && options.fallback != "skip" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid fallback mode {:?}", options.fallback),
        ));
    }
    Ok(())
}

fn source_relative_path(options: &Options, source: &Path) -> PathBuf {
    let source = source
        .canonicalize()
        .unwrap_or_else(|_| source.to_path_buf());
    if options.recursive {
        let mut roots: Vec<_> = options
            .inputs
            .iter()
            .filter_map(|input| input.canonicalize().ok())
            .filter(|input| input.is_dir())
            .filter_map(|input| {
                source
                    .strip_prefix(&input)
                    .ok()
                    .map(|relative| (input, relative.to_owned()))
            })
            .collect();
        roots.sort_by_key(|(root, _)| root.components().count());
        if let Some((_, relative)) = roots.pop() {
            return relative;
        }
    }
    source
        .file_name()
        .map_or_else(|| PathBuf::from("nif.map"), PathBuf::from)
}

fn build_jobs(options: &Options, inputs: Vec<PathBuf>) -> Vec<(PathBuf, PathBuf, PathBuf)> {
    let mut output_counts = HashMap::<String, usize>::new();
    for input in &inputs {
        let relative = source_relative_path(options, input).with_extension("map");
        *output_counts
            .entry(relative.to_string_lossy().to_ascii_lowercase())
            .or_default() += 1;
    }
    inputs
        .into_iter()
        .map(|source| {
            let mut relative = source_relative_path(options, &source).with_extension("map");
            if output_counts
                .get(&relative.to_string_lossy().to_ascii_lowercase())
                .copied()
                .unwrap_or(0)
                > 1
            {
                let stem = relative
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("nif");
                relative.set_file_name(format!("{stem}__{}.map", sha1_suffix(&source)));
            }
            let output = options.output_dir.join(relative);
            let report = output.with_extension("nif2map.json");
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
    batch_elapsed: Duration,
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
                    let timings = report.timings;
                    println!(
                        "      timing: parse/semantic {:.3} ms, texture resolution {:.3} ms",
                        timings.parse_semantic.as_secs_f64() * 1000.0,
                        timings.texture_resolution.as_secs_f64() * 1000.0
                    );
                    println!(
                        "      timing: sweep {:.3} ms, structural {:.3} ms, planar fallback {:.3} ms",
                        timings.sweep_analysis.as_secs_f64() * 1000.0,
                        timings.structural_recognition.as_secs_f64() * 1000.0,
                        timings.planar_fallback.as_secs_f64() * 1000.0
                    );
                    println!(
                        "      timing: validation {:.3} ms, UV diagnostics {:.3} ms, map serialization {:.3} ms, report serialization {:.3} ms, TOTAL {:.3} ms",
                        timings.brush_validation.as_secs_f64() * 1000.0,
                        timings.uv_diagnostics.as_secs_f64() * 1000.0,
                        timings.map_serialization.as_secs_f64() * 1000.0,
                        timings.report_serialization.as_secs_f64() * 1000.0,
                        timings.total.as_secs_f64() * 1000.0
                    );
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
    if options.verbose {
        println!("batch wall time: {:.3} s", batch_elapsed.as_secs_f64());
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
mod tests;
