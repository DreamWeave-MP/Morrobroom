use std::fs;
use std::path::Path;

use crate::slipgate::repr::Map;

use crate::slipgate::{
    GeoMap, brush, face,
    face::{
        FaceCenters, FaceNormals, FacePlanes, FaceTriangleIndices, FaceVertices, OccludedFaces,
    },
    line,
    texture::TextureSizes,
};

// ── Error ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum MapGeometryError {
    /// The map file could not be read from disk.
    Io(std::io::Error),
    /// The map file contents could not be parsed. Contains a description of
    /// where in the input the parser failed.
    Parse(String),
}

impl std::fmt::Display for MapGeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for MapGeometryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(_) => None,
        }
    }
}

impl From<std::io::Error> for MapGeometryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// ── MapGeometry ───────────────────────────────────────────────────────────────

/// All geometry derived from a Quake `.map` file, computed in one step.
///
/// Construct with [`MapGeometry::new`], then call [`MapGeometry::face_uvs`]
/// separately once you have texture size data.
pub struct MapGeometry {
    /// The raw struct-of-arrays map representation. Exposes entity properties,
    /// texture names, brush/face topology, and face plane data.
    pub geomap: GeoMap,
    /// Plane equation for each face, derived from the three defining points.
    pub face_planes: FacePlanes,
    /// World-space vertices for each face, computed via triplanar intersection.
    pub face_vertices: FaceVertices,
    /// Centroid of each face's vertex set.
    pub face_centers: FaceCenters,
    /// Triangle indices for each face in clockwise winding order.
    pub face_tri_indices: FaceTriangleIndices,
    /// Triangle indices for each face in counter-clockwise winding order,
    /// for brushes tagged as inside-out by the map author.
    pub inverted_face_tri_indices: FaceTriangleIndices,
    /// Per-vertex flat normals (copied from the face plane normal).
    pub flat_normals: FaceNormals,
    /// Per-vertex smooth normals (averaged across the three generating planes).
    pub smooth_normals: FaceNormals,
    /// Faces that are completely hidden by other solid geometry and can be
    /// skipped during mesh generation.
    ///
    /// This is `None` when the geometry was built with
    /// [`MapGeometry::from_map_without_occlusion`]. An empty set therefore
    /// means that occlusion was computed and no faces were hidden.
    pub occluded_faces: Option<OccludedFaces>,
}

impl MapGeometry {
    /// Read and fully process a `.map` file.
    ///
    /// Both winding orders are computed and stored since map authors may tag
    /// individual brushes as inside-out. For UV coordinates call
    /// [`face_uvs`](Self::face_uvs) separately.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, MapGeometryError> {
        let map_string = fs::read_to_string(path)?;
        let map = map_string
            .parse::<Map>()
            .map_err(|e| MapGeometryError::Parse(format!("at '{}' ({:?})", e.input, e.code)))?;

        Ok(Self::from_map(map))
    }

    /// Process an already parsed map without performing filesystem I/O.
    ///
    /// This is the full constructor and includes the complete-face occlusion
    /// pass. Call [`MapGeometry::from_map_without_occlusion`] when the caller
    /// only needs the core geometry pipeline.
    pub fn from_map(map: Map) -> Self {
        Self::build(map, true)
    }

    /// Process an already parsed map without running the occlusion pass.
    ///
    /// The core geometry stages are identical to [`MapGeometry::from_map`],
    /// but the quadratic occlusion scans are skipped. This is the constructor
    /// intended for frontends that do not consume complete-face occlusion.
    pub fn from_map_without_occlusion(map: Map) -> Self {
        Self::build(map, false)
    }

    fn build(map: Map, compute_occlusion: bool) -> Self {
        let geomap = GeoMap::new(map);

        // ── Core geometry ─────────────────────────────────────────────────────
        let face_planes = face::face_planes(&geomap.face_planes);
        let brush_hulls = brush::brush_hulls(&geomap.brush_faces, &face_planes);
        let (face_vertices, face_vertex_planes) =
            face::face_vertices(&geomap.brush_faces, &face_planes, &brush_hulls);
        let face_centers = face::face_centers(&face_vertices);

        let (face_indices_cw, face_indices_ccw) = face::face_indices_both(
            &geomap.face_planes,
            &face_planes,
            &face_vertices,
            &face_centers,
        );

        let face_tri_indices = face::face_triangle_indices(&face_indices_cw);
        let inverted_face_tri_indices = face::face_triangle_indices(&face_indices_ccw);
        let flat_normals = face::normals_flat(&face_vertices, &face_planes);
        let smooth_normals = face::normals_phong_averaged(&face_vertex_planes, &face_planes);

        // ── Occlusion ─────────────────────────────────────────────────────────
        // Do not pay for the quadratic scans when the caller only needs core
        // geometry. `Some(empty)` remains meaningful: it means the pass ran.
        let occluded_faces = compute_occlusion.then(|| {
            let face_duplicates =
                face::face_duplicates(&geomap.faces, &face_planes, &face_vertices);
            let brush_face_containment = brush::brush_face_containment(
                &geomap.brushes,
                &geomap.faces,
                &geomap.brush_faces,
                &brush_hulls,
                &face_vertices,
            );
            let face_bases = face::face_bases(&geomap.faces, &face_planes, &geomap.face_offsets);
            // Edge topology is winding-independent; CW indices are sufficient here.
            let (lines, face_lines) = line::lines(&face_indices_cw);
            let face_face_containment = face::face_face_containment(
                &geomap.faces,
                &lines,
                &face_planes,
                &face_bases,
                &face_vertices,
                &face_lines,
            );
            face::occluded_faces(
                &face_duplicates,
                &brush_face_containment,
                &face_face_containment,
            )
        });

        MapGeometry {
            geomap,
            face_planes,
            face_vertices,
            face_centers,
            face_tri_indices,
            inverted_face_tri_indices,
            flat_normals,
            smooth_normals,
            occluded_faces,
        }
    }

    /// Compute UV coordinates for all faces.
    ///
    /// Separated from construction because texture sizes must come from outside
    /// the map file (typically loaded from the game's asset pipeline).
    pub fn face_uvs(&self, texture_sizes: TextureSizes) -> face::FaceUvs {
        face::new(
            &self.geomap.faces,
            &self.geomap.textures,
            &self.geomap.face_textures,
            &self.face_vertices,
            &self.face_planes,
            &self.geomap.face_offsets,
            &self.geomap.face_angles,
            &self.geomap.face_scales,
            &texture_sizes,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::slipgate::repr::Map;

    use super::*;

    const CUBE_MAP: &str = r#"{
"classname" "worldspawn"
{
( -32 -16 -16 ) ( -32 -15 -16 ) ( -32 -16 -15 ) normal/tile0_floor_ornate 0 0 0 0.25 0.25 0
( -16 -32 -16 ) ( -16 -32 -15 ) ( -15 -32 -16 ) normal/tile0_floor_ornate 0 0 0 0.25 0.25 0
( -16 -16 -32 ) ( -15 -16 -32 ) ( -16 -15 -32 ) normal/tile0_floor_ornate 0 0 0 0.25 0.25 0
( 16 16 32 ) ( 16 17 32 ) ( 17 16 32 ) normal/tile0_floor_ornate 64 64 -3.8147e-06 0.25 0.25 0
( 16 32 32 ) ( 17 32 32 ) ( 16 32 33 ) normal/tile0_floor_ornate 0 0 0 0.25 0.25 0
( 32 16 32 ) ( 32 16 33 ) ( 32 17 32 ) normal/tile0_floor_ornate 0 0 0 0.25 0.25 0
}
}
"#;

    #[test]
    fn map_geometry_preserves_the_core_cube_pipeline() {
        let map = CUBE_MAP.parse::<Map>().expect("cube fixture should parse");
        let geometry = MapGeometry::from_map(map);

        assert_eq!(geometry.geomap.entities.len(), 1);
        assert_eq!(geometry.geomap.brushes.len(), 1);
        assert_eq!(geometry.geomap.faces.len(), 6);
        assert!(
            geometry
                .occluded_faces
                .as_ref()
                .expect("full construction computes occlusion")
                .is_empty()
        );

        for face_id in geometry.geomap.faces.iter() {
            let vertices = &geometry.face_vertices[face_id];
            assert_eq!(vertices.len(), 4, "face {face_id:?}");
            assert_eq!(geometry.face_tri_indices[face_id].len(), 6);
            assert_eq!(geometry.inverted_face_tri_indices[face_id].len(), 6);
            assert_eq!(geometry.flat_normals[face_id].len(), vertices.len());
            assert_eq!(geometry.smooth_normals[face_id].len(), vertices.len());
            assert!(
                vertices
                    .iter()
                    .all(|vertex| vertex.iter().all(|value| value.is_finite()))
            );
        }
    }

    #[test]
    fn map_geometry_reports_shared_internal_faces() {
        let map = include_str!("../../tests/fixtures/maps/two_touching_brushes.map")
            .parse::<Map>()
            .expect("touching-brush fixture should parse");
        let geometry = MapGeometry::from_map(map);

        assert_eq!(geometry.geomap.brushes.len(), 2);
        assert_eq!(geometry.geomap.faces.len(), 8);
        assert_eq!(
            geometry
                .occluded_faces
                .as_ref()
                .expect("full construction computes occlusion")
                .len(),
            2
        );
    }

    #[test]
    fn map_geometry_can_skip_occlusion_without_changing_core_geometry() {
        let full = MapGeometry::from_map(CUBE_MAP.parse().expect("cube fixture should parse"));
        let geometry = MapGeometry::from_map_without_occlusion(
            CUBE_MAP.parse().expect("cube fixture should parse"),
        );

        assert!(geometry.occluded_faces.is_none());
        assert_eq!(geometry.geomap.faces.len(), 6);
        for face_id in geometry.geomap.faces.iter() {
            assert_eq!(geometry.face_vertices[face_id].len(), 4);
            assert_eq!(geometry.face_tri_indices[face_id].len(), 6);
            assert_eq!(geometry.face_vertices[face_id], full.face_vertices[face_id]);
            assert_eq!(
                geometry.face_tri_indices[face_id],
                full.face_tri_indices[face_id]
            );
        }
    }
}
