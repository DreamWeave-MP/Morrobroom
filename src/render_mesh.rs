use crate::slipgate::{
    Vector2, Vector3,
    csg::GeometryTolerance,
    face::FaceId,
    map_geometry::{InvalidFacePolygon, MapGeometry},
    texture::{TextureId, TextureSizes},
};
use crate::slipgate::{brush::BrushId, repr::Extension};

/// The material policy needed by the compiler before it reaches the NIF
/// backend. It deliberately contains no TES3 objects.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMaterial {
    pub texture_id: TextureId,
    pub texture: String,
    pub extension: Extension,
    pub content_flags: u32,
    pub surface_flags: u32,
    pub collision: CollisionPolicy,
    pub shading: ShadingPolicy,
    pub invert_winding: bool,
    pub emissive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    Solid,
    NoClip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadingPolicy {
    Flat,
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderVertex {
    pub position: Vector3,
    pub normal: Vector3,
    pub uv: Vector2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPart {
    pub source_face: FaceId,
    pub source_brush: BrushId,
    pub material: RenderMaterial,
    pub vertices: Vec<RenderVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct RenderMesh {
    pub parts: Vec<RenderPart>,
}

impl RenderMesh {
    /// Compile exact CSG fragments into backend-neutral render geometry.
    ///
    /// UVs are evaluated from source-face metadata at every emitted vertex;
    /// they are not interpolated from the original polygon. This is the
    /// important property that makes Valve 220 projection correct after
    /// clipping invents vertices.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidFacePolygon`] if a source face cannot form a valid CSG
    /// polygon.
    ///
    /// # Panics
    ///
    /// Panics if a single render part exceeds the NIF backend's `u32` index
    /// space.
    pub fn from_geometry(
        geometry: &MapGeometry,
        texture_sizes: &TextureSizes,
        tolerance: GeometryTolerance,
    ) -> Result<Self, InvalidFacePolygon> {
        let fragments = geometry.visible_face_fragments(tolerance)?;
        let mut mesh = Self::default();

        for fragment in fragments {
            let face_id = fragment.source_face;
            let texture_id = geometry.geomap.face_textures[face_id];
            let texture = geometry.geomap.textures[texture_id].clone();
            if texture == "clip" || texture == "skip" || texture.contains("skip_") {
                continue;
            }

            let (content_flags, surface_flags) = match &geometry.geomap.face_extensions[face_id] {
                Extension::Quake2 {
                    content_flags,
                    surface_flags,
                    ..
                } => (*content_flags, *surface_flags),
                _ => (0, 0),
            };
            let liquid = {
                let lower = texture.to_ascii_lowercase();
                lower.contains("slime")
                    || lower.contains("water")
                    || lower.contains("lava")
                    || lower.contains("mwat")
            };
            let no_clip = surface_flags & 1 != 0 || liquid;
            let invert_winding = surface_flags & 4 != 0;
            let shading = if surface_flags & 2 != 0 {
                ShadingPolicy::Smooth
            } else {
                ShadingPolicy::Flat
            };
            let material = RenderMaterial {
                texture_id,
                texture,
                extension: geometry.geomap.face_extensions[face_id].clone(),
                content_flags,
                surface_flags,
                collision: if no_clip {
                    CollisionPolicy::NoClip
                } else {
                    CollisionPolicy::Solid
                },
                shading,
                invert_winding,
                emissive: geometry.geomap.textures[texture_id].eq_ignore_ascii_case("sky5_blu"),
            };

            let outward_normal = *geometry.face_planes[face_id].normal();
            let fragment_normal = polygon_normal(&fragment.polygon.vertices, outward_normal);
            let reversed = fragment_normal.dot(&outward_normal) < 0.0;
            let reverse_indices = reversed ^ invert_winding;
            let texture_size = texture_sizes
                .get(&texture_id)
                .copied()
                .unwrap_or((256, 256));
            let vertices = fragment
                .polygon
                .vertices
                .iter()
                .map(|position| {
                    let position = position.map(crate::slipgate::f64_to_f32);
                    let uv = crate::slipgate::face::vertex_uv(
                        position,
                        geometry.face_planes[face_id],
                        geometry.geomap.face_offsets[face_id],
                        geometry.geomap.face_angles[face_id],
                        geometry.geomap.face_scales[face_id],
                        nalgebra::vector![
                            crate::slipgate::u32_to_f32(texture_size.0),
                            crate::slipgate::u32_to_f32(texture_size.1)
                        ],
                    );
                    RenderVertex {
                        position,
                        normal: outward_normal,
                        uv,
                    }
                })
                .collect::<Vec<_>>();
            let mut indices = Vec::with_capacity((vertices.len() - 2) * 3);
            for index in 1..(vertices.len() - 1) {
                let triangle = if reverse_indices {
                    [0, index + 1, index]
                } else {
                    [0, index, index + 1]
                };
                indices.extend(triangle.into_iter().map(|index| {
                    u32::try_from(index).expect("render mesh exceeds u32 index space")
                }));
            }
            mesh.parts.push(RenderPart {
                source_face: fragment.source_face,
                source_brush: fragment.source_brush,
                material,
                vertices,
                indices,
            });
        }

        Ok(mesh)
    }
}

fn polygon_normal(vertices: &[crate::slipgate::csg::CsgVector], fallback: Vector3) -> Vector3 {
    let mut normal = crate::slipgate::csg::CsgVector::zeros();
    for (current, next) in vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
    {
        normal += current.cross(next);
    }
    let normal = normal.map(crate::slipgate::f64_to_f32);
    if normal.norm_squared() <= f32::EPSILON {
        fallback
    } else {
        normal.normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slipgate::{csg::GeometryTolerance, repr::Map};
    use std::collections::BTreeMap;

    #[test]
    fn clipped_vertices_receive_source_projection_and_provenance() {
        const SMALL_OVERLAP: &str = r#"// Game: Qodot
// Format: Valve
{
"classname" "worldspawn"
"mapversion" "220"
{
( -16 -16 -16 ) ( -16 -15 -16 ) ( -16 -16 -15 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( -16 -16 -16 ) ( -16 -16 -15 ) ( -15 -16 -16 ) mb/canonical [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -16 -16 -16 ) ( -15 -16 -16 ) ( -16 -15 -16 ) mb/canonical [ -1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 16 17 16 ) ( 17 16 16 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 16 16 16 ) ( 17 16 16 ) ( 16 16 17 ) mb/canonical [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 16 16 16 ) ( 16 16 17 ) ( 16 17 16 ) mb/canonical [ 0 -1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
{
( -8 -16 -16 ) ( -8 -15 -16 ) ( -8 -16 -15 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( -8 -16 -16 ) ( -8 -16 -15 ) ( -7 -16 -16 ) mb/canonical [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -8 -16 -16 ) ( -7 -16 -16 ) ( -8 -15 -16 ) mb/canonical [ -1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 24 16 16 ) ( 24 17 16 ) ( 25 16 16 ) mb/canonical [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 24 16 16 ) ( 25 16 16 ) ( 24 16 17 ) mb/canonical [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 24 16 16 ) ( 24 16 17 ) ( 24 17 16 ) mb/canonical [ 0 -1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
"#;
        let map = SMALL_OVERLAP
            .parse::<Map>()
            .expect("small overlap fixture should parse");
        let geometry = MapGeometry::from_map_without_occlusion(map);
        let sizes = crate::slipgate::texture::texture_sizes(
            &geometry.geomap.textures,
            &BTreeMap::from([("mb/canonical", (256, 256))]),
        );
        let mesh = RenderMesh::from_geometry(&geometry, &sizes, GeometryTolerance::default())
            .expect("small overlap faces should compile");
        assert!(!mesh.parts.is_empty());
        assert!(mesh.parts.iter().all(|part| {
            !part.vertices.is_empty()
                && !part.indices.is_empty()
                && part
                    .indices
                    .iter()
                    .all(|index| (*index as usize) < part.vertices.len())
        }));
        assert!(mesh.parts.iter().all(|part| {
            part.source_face.0 < geometry.geomap.faces.len()
                && part.source_brush.0 < geometry.geomap.brushes.len()
        }));
        assert!(mesh.parts.iter().any(|part| {
            part.vertices.iter().any(|vertex| {
                !geometry.face_polygons[part.source_face]
                    .iter()
                    .any(|source| {
                        (source.x - vertex.position.x).abs() <= f32::EPSILON
                            && (source.y - vertex.position.y).abs() <= f32::EPSILON
                            && (source.z - vertex.position.z).abs() <= f32::EPSILON
                    })
            })
        }));

        for part in &mesh.parts {
            let expected = *geometry.face_planes[part.source_face].normal();
            for triangle in part.indices.chunks_exact(3) {
                let a = part.vertices[triangle[0] as usize].position;
                let b = part.vertices[triangle[1] as usize].position;
                let c = part.vertices[triangle[2] as usize].position;
                let geometric_normal = (b - a).cross(&(c - a));
                let dot = geometric_normal.dot(&expected);
                assert!(
                    if part.material.invert_winding {
                        dot < -f32::EPSILON
                    } else {
                        dot > f32::EPSILON
                    },
                    "render triangle winding disagrees with its canonical face normal"
                );
            }
        }
    }
}
