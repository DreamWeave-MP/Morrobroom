use std::{fs::File, io, io::Write, path::Path};

use lightmap::input::WorldVertex;

use crate::{
    render_mesh::{RenderMesh, RenderVertex},
    slipgate::Vector2,
};

/// A render vertex carrying the generated second UV channel.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedVertex {
    pub render: RenderVertex,
    pub lightmap_uv: Vector2,
}

/// Render geometry after lightmap UV generation and seam vertex duplication.
///
/// Triangle order is intentionally preserved from [`RenderMesh`]. This lets
/// the NIF backend retain material and provenance information while replacing
/// the geometry's topology with the UV generator's topology.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedRenderMesh {
    pub vertices: Vec<BakedVertex>,
    pub triangles: Vec<[u32; 3]>,
    pub triangle_parts: Vec<usize>,
    pub parts: Vec<BakedPart>,
}

/// One source-material partition after lightmap UV generation.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedPart {
    pub vertices: Vec<BakedVertex>,
    pub indices: Vec<u32>,
}

/// Write a baked RGB lightmap as an uncompressed TGA accepted by `OpenMW`.
///
/// # Errors
///
/// Returns an error if the lightmap dimensions exceed the TGA limits, the
/// pixel buffer is malformed, or the destination cannot be written.
pub fn write_tga(path: &Path, lightmap: &lightmap::LightMap) -> io::Result<()> {
    let width = u16::try_from(lightmap.width)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lightmap is wider than TGA"))?;
    let height = u16::try_from(lightmap.height)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lightmap is taller than TGA"))?;
    let expected_len = lightmap.width * lightmap.height * 3;
    if lightmap.pixels.len() != expected_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "lightmap RGB data has the wrong size",
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = File::create(path)?;
    let mut header = [0_u8; 18];
    header[2] = 2;
    header[12..14].copy_from_slice(&width.to_le_bytes());
    header[14..16].copy_from_slice(&height.to_le_bytes());
    header[16] = 24;
    file.write_all(&header)?;
    for pixel in lightmap.pixels.chunks_exact(3) {
        file.write_all(&[pixel[2], pixel[1], pixel[0]])?;
    }
    Ok(())
}

impl BakedRenderMesh {
    /// Generate a second UV channel from render geometry.
    ///
    /// The UV generator owns seam splitting and chart packing. Morrobroom
    /// copies the complete [`RenderVertex`] when it asks for a seam clone, so
    /// normals, base UVs, and the `RenderMesh` boundary remain aligned with the
    /// generated topology.
    #[must_use]
    pub fn from_render_mesh(render_mesh: &RenderMesh, spacing: f32) -> Option<Self> {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        let mut triangle_parts = Vec::new();

        for (part_index, part) in render_mesh.parts.iter().enumerate() {
            if !part.indices.chunks_exact(3).remainder().is_empty() {
                return None;
            }
            let vertex_offset = u32::try_from(vertices.len()).ok()?;
            vertices.extend(part.vertices.iter().copied().map(|render| BakedVertex {
                render,
                lightmap_uv: Vector2::zeros(),
            }));
            for indices in part.indices.chunks_exact(3) {
                let triangle = [
                    indices[0].checked_add(vertex_offset)?,
                    indices[1].checked_add(vertex_offset)?,
                    indices[2].checked_add(vertex_offset)?,
                ];
                triangles.push(triangle);
                triangle_parts.push(part_index);
            }
        }

        let positions = vertices.iter().map(|vertex| vertex.render.position);
        let patch = lightmap::uvgen::generate_uvs(positions, triangles.iter().copied(), spacing)?;

        for &source_index in &patch.additional_vertices {
            let source = vertices.get(source_index as usize)?.clone();
            vertices.push(source);
        }

        if patch.second_tex_coords.len() != vertices.len()
            || patch.triangles.len() != triangle_parts.len()
        {
            return None;
        }

        for (vertex, &lightmap_uv) in vertices.iter_mut().zip(&patch.second_tex_coords) {
            vertex.lightmap_uv = lightmap_uv;
        }

        if patch
            .triangles
            .iter()
            .flatten()
            .any(|&index| index as usize >= vertices.len())
        {
            return None;
        }

        let mut parts = (0..render_mesh.parts.len())
            .map(|_| BakedPart {
                vertices: Vec::new(),
                indices: Vec::new(),
            })
            .collect::<Vec<_>>();
        let mut local_indices =
            vec![std::collections::HashMap::<u32, u32>::new(); render_mesh.parts.len()];
        for (triangle_index, triangle) in patch.triangles.iter().enumerate() {
            let part_index = *triangle_parts.get(triangle_index)?;
            let part = parts.get_mut(part_index)?;
            let local_map = local_indices.get_mut(part_index)?;
            for &global_index in triangle {
                let local_index = if let Some(&local_index) = local_map.get(&global_index) {
                    local_index
                } else {
                    let local_index = u32::try_from(part.vertices.len()).ok()?;
                    part.vertices
                        .push(vertices.get(global_index as usize)?.clone());
                    local_map.insert(global_index, local_index);
                    local_index
                };
                part.indices.push(local_index);
            }
        }

        Some(Self {
            vertices,
            triangles: patch.triangles,
            triangle_parts,
            parts,
        })
    }

    /// Convert the generated geometry to the lightmap crate's input mesh.
    #[must_use]
    pub fn input_mesh(&self) -> Option<lightmap::input::Mesh> {
        let vertices = self
            .vertices
            .iter()
            .map(|vertex| WorldVertex {
                world_normal: vertex.render.normal,
                world_position: vertex.render.position,
                second_tex_coord: vertex.lightmap_uv,
            })
            .collect();
        lightmap::input::Mesh::new(vertices, self.triangles.clone())
    }

    #[must_use]
    pub fn part(&self, index: usize) -> Option<&BakedPart> {
        self.parts.get(index)
    }

    /// Bake one lightmap for this render mesh against the supplied scene.
    #[must_use]
    pub fn bake(
        &self,
        other_meshes: &[lightmap::input::Mesh],
        lights: &[lightmap::light::LightDefinition],
        texels_per_unit: usize,
    ) -> Option<lightmap::LightMap> {
        let mesh = self.input_mesh()?;
        Some(lightmap::LightMap::new(
            &mesh,
            other_meshes,
            lights,
            texels_per_unit,
        ))
    }
}

#[cfg(test)]
mod tests {
    use lightmap::light::{LightDefinition, PointLightDefinition};
    use nalgebra::{Vector2, Vector3};

    use super::*;
    use crate::{
        render_mesh::{CollisionPolicy, RenderMaterial, RenderPart, ShadingPolicy},
        slipgate::{brush::BrushId, face::FaceId, repr::Extension},
    };

    fn cube_mesh() -> RenderMesh {
        let positions = [
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
        ];
        let material = RenderMaterial {
            texture_id: crate::slipgate::texture::TextureId(0),
            texture: "probe".to_owned(),
            extension: Extension::Standard,
            content_flags: 0,
            surface_flags: 0,
            collision: CollisionPolicy::Solid,
            shading: ShadingPolicy::Flat,
            invert_winding: false,
            emissive: false,
        };

        let vertices = positions
            .into_iter()
            .map(|position| {
                let position = Vector3::from(position);
                RenderVertex {
                    normal: position.normalize(),
                    position,
                    uv: Vector2::zeros(),
                }
            })
            .collect();
        RenderMesh {
            parts: vec![RenderPart {
                source_face: FaceId(0),
                source_brush: BrushId(0),
                material,
                vertices,
                indices: vec![
                    2, 1, 0, 3, 2, 0, 4, 5, 6, 4, 6, 7, 7, 6, 2, 2, 3, 7, 0, 1, 5, 0, 5, 4, 5, 1,
                    2, 5, 2, 6, 3, 0, 4, 7, 3, 4,
                ],
            }],
        }
    }

    #[test]
    fn uv_generation_clones_complete_render_vertices() {
        let source = cube_mesh();
        let baked = BakedRenderMesh::from_render_mesh(&source, 0.005)
            .expect("cube UV generation should succeed");

        assert!(baked.vertices.len() > 24);
        assert_eq!(baked.triangles.len(), 12);
        assert_eq!(baked.triangle_parts.len(), baked.triangles.len());
        assert!(
            baked
                .vertices
                .iter()
                .any(|vertex| vertex.lightmap_uv != Vector2::zeros())
        );
        assert!(baked.vertices.iter().all(|vertex| {
            source.parts.iter().any(|part| {
                part.vertices.iter().any(|source_vertex| {
                    source_vertex.position == vertex.render.position
                        && source_vertex.normal == vertex.render.normal
                })
            })
        }));
    }

    #[test]
    fn lightmap_bake_produces_rgb_pixels() {
        let baked = BakedRenderMesh::from_render_mesh(&cube_mesh(), 0.005)
            .expect("cube UV generation should succeed");
        let input = baked.input_mesh().expect("baked mesh should be valid");
        let lights = [LightDefinition::Point(PointLightDefinition {
            intensity: 1.0,
            color: Vector3::new(1.0, 1.0, 1.0),
            radius: 8.0,
            position: Vector3::new(0.0, 0.0, 4.0),
            sqr_radius: 64.0,
        })];
        let meshes = vec![input];
        let lightmap = lightmap::LightMap::new(&meshes[0], &meshes, &lights, 8);

        assert!(lightmap.width > 0);
        assert_eq!(lightmap.pixels.len(), lightmap.width * lightmap.height * 3);
        assert!(lightmap.pixels.iter().any(|&pixel| pixel != 0));
    }
}
