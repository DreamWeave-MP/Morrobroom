use std::{fs::File, io, path::Path};

use lightmap::input::WorldVertex;
use rusty_dds::{Dds, DecodeContent, EncodeLayout};

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

const MAX_LIGHTMAP_DIMENSION: usize = 8192;

/// Prepare the baked atlas for the BC7/OpenMW texture path.
///
/// `OpenSceneGraph`'s NIF texture path attempts to resize non-power-of-two
/// images before upload, but cannot resize an already-compressed BC7 image.
/// Keep the final atlas power-of-two so the runtime accepts it, while also
/// retaining the existing 8K dimension limit.
///
/// # Panics
///
/// Panics if the source atlas has zero dimensions or malformed RGB data.
#[must_use]
pub fn constrain_lightmap(lightmap: lightmap::LightMap) -> lightmap::LightMap {
    assert!(lightmap.width > 0 && lightmap.height > 0);
    assert_eq!(
        lightmap.pixels.len(),
        lightmap.width * lightmap.height * 3,
        "lightmap RGB data must match its dimensions"
    );
    let width = normalized_dimension(lightmap.width);
    let height = normalized_dimension(lightmap.height);
    if width == lightmap.width && height == lightmap.height {
        return lightmap;
    }

    resample_lightmap(&lightmap, width, height)
}

fn normalized_dimension(value: usize) -> usize {
    value
        .checked_next_power_of_two()
        .unwrap_or(MAX_LIGHTMAP_DIMENSION)
        .clamp(1, MAX_LIGHTMAP_DIMENSION)
}

fn resample_lightmap(
    lightmap: &lightmap::LightMap,
    width: usize,
    height: usize,
) -> lightmap::LightMap {
    let mut pixels = vec![0_u8; width * height * 3];

    for y in 0..height {
        let (y0, y1, y_remainder, y_denominator) = interpolation_axis(y, height, lightmap.height);
        for x in 0..width {
            let (x0, x1, x_remainder, x_denominator) = interpolation_axis(x, width, lightmap.width);
            let top_left = (y0 * lightmap.width + x0) * 3;
            let top_right = (y0 * lightmap.width + x1) * 3;
            let bottom_left = (y1 * lightmap.width + x0) * 3;
            let bottom_right = (y1 * lightmap.width + x1) * 3;
            let output_offset = (y * width + x) * 3;
            for channel in 0..3 {
                let top = u64::from(lightmap.pixels[top_left + channel])
                    * (x_denominator - x_remainder)
                    + u64::from(lightmap.pixels[top_right + channel]) * x_remainder;
                let bottom = u64::from(lightmap.pixels[bottom_left + channel])
                    * (x_denominator - x_remainder)
                    + u64::from(lightmap.pixels[bottom_right + channel]) * x_remainder;
                let numerator = top * (y_denominator - y_remainder) + bottom * y_remainder;
                let denominator = x_denominator * y_denominator;
                pixels[output_offset + channel] =
                    u8::try_from((numerator + denominator / 2) / denominator)
                        .expect("bilinear RGB sample must fit in a byte");
            }
        }
    }

    lightmap::LightMap {
        pixels,
        width,
        height,
    }
}

/// Add a uniform interior-ambient floor to the baked RGB lightmap.
///
/// `lightmap` produces direct-light-only pixels, so an occluded texel is
/// otherwise exactly black. The authored Morrowind interior ambient color is
/// an independent baseline and is applied in LDR space here because the
/// upstream crate does not expose its per-texel accumulation step.
pub fn add_ambient(lightmap: &mut lightmap::LightMap, ambient: [u8; 3]) {
    for pixel in lightmap.pixels.chunks_exact_mut(3) {
        for (channel, value) in pixel.iter_mut().enumerate() {
            *value = value.saturating_add(ambient[channel]);
        }
    }
}

fn interpolation_axis(index: usize, target: usize, source: usize) -> (usize, usize, u64, u64) {
    if target <= 1 || source <= 1 {
        return (0, 0, 0, 1);
    }
    let denominator = u64::try_from(target - 1).expect("texture dimension must fit in u64");
    let numerator = u64::try_from(index).expect("texture coordinate must fit in u64")
        * u64::try_from(source - 1).expect("texture dimension must fit in u64");
    let lower = usize::try_from(numerator / denominator)
        .expect("source texture coordinate must fit in usize");
    let upper = (lower + 1).min(source - 1);
    (lower, upper, numerator % denominator, denominator)
}

fn expand_rgb_to_rgba(lightmap: &lightmap::LightMap) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(lightmap.width * lightmap.height * 4);
    for source in lightmap.pixels.chunks_exact(3) {
        rgba.extend_from_slice(&[source[0], source[1], source[2], u8::MAX]);
    }
    rgba
}

/// Write a baked RGB lightmap as a BC7-compressed DDS accepted by `OpenMW`.
///
/// # Errors
///
/// Returns an error if the lightmap is malformed or the destination cannot be
/// written.
pub fn write_dds(path: &Path, lightmap: &lightmap::LightMap) -> io::Result<()> {
    let width = u32::try_from(lightmap.width)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lightmap is too wide"))?;
    let height = u32::try_from(lightmap.height)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lightmap is too tall"))?;
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

    // UV generation and LightMap::new use the same row order. OpenMW's DDS
    // loader consumes the payload without a second vertical flip, so preserve
    // the atlas rows while expanding RGB to opaque RGBA for BC7.
    let rgba = expand_rgb_to_rgba(lightmap);

    let layout = EncodeLayout::flat_2d(DecodeContent::Bc7, width, height);
    let dds = Dds::encode_from_rgba8(&rgba, layout)
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut file = File::create(path)?;
    dds.write(&mut file)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok(())
}

impl BakedRenderMesh {
    /// Generate a second UV channel from render geometry.
    ///
    /// The UV generator owns seam splitting and chart packing. Morrobroom
    /// copies the complete [`RenderVertex`] when it asks for a seam clone, so
    /// normals, base UVs, and the `RenderMesh` boundary remain aligned with the
    /// generated topology.
    ///
    /// # Panics
    ///
    /// Panics if the UV generator changes source triangle order or winding.
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

        if patch
            .triangles
            .iter()
            .flatten()
            .any(|&index| index as usize >= vertices.len())
        {
            return None;
        }
        assert_uvgen_preserves_topology(&vertices, &triangles, &patch.triangles);

        for (vertex, &lightmap_uv) in vertices.iter_mut().zip(&patch.second_tex_coords) {
            vertex.lightmap_uv = lightmap_uv;
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

fn assert_uvgen_preserves_topology(
    vertices: &[BakedVertex],
    source_triangles: &[[u32; 3]],
    generated_triangles: &[[u32; 3]],
) {
    for (generated, source) in generated_triangles.iter().zip(source_triangles) {
        assert!(
            generated.iter().zip(source).all(|(&generated, &source)| {
                vertices[generated as usize].render.position
                    == vertices[source as usize].render.position
            }),
            "lightmap UV generation reordered or reversed source triangles"
        );

        let source_a = vertices[source[0] as usize].render.position;
        let source_b = vertices[source[1] as usize].render.position;
        let source_c = vertices[source[2] as usize].render.position;
        let source_normal = (source_b - source_a).cross(&(source_c - source_a));
        let generated_a = vertices[generated[0] as usize].render.position;
        let generated_b = vertices[generated[1] as usize].render.position;
        let generated_c = vertices[generated[2] as usize].render.position;
        assert!(
            (generated_b - generated_a)
                .cross(&(generated_c - generated_a))
                .dot(&source_normal)
                > f32::EPSILON,
            "lightmap UV generation changed triangle winding"
        );
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
        assert_eq!(baked.triangle_parts, vec![0; 12]);
        assert_eq!(baked.parts.len(), 1);
        assert_eq!(baked.parts[0].indices.len(), source.parts[0].indices.len());
    }

    #[test]
    fn uv_generation_preserves_material_provenance() {
        let mut source = cube_mesh();
        let mut second = source.parts[0].clone();
        for vertex in &mut second.vertices {
            vertex.position.x += 4.0;
        }
        second.source_face = FaceId(1);
        second.source_brush = BrushId(1);
        source.parts.push(second);

        let baked = BakedRenderMesh::from_render_mesh(&source, 0.005)
            .expect("two-part cube UV generation should succeed");

        assert_eq!(baked.triangles.len(), 24);
        assert_eq!(&baked.triangle_parts[..12], &[0; 12]);
        assert_eq!(&baked.triangle_parts[12..], &[1; 12]);
        assert_eq!(baked.parts.len(), 2);
        assert_eq!(baked.parts[0].indices.len(), source.parts[0].indices.len());
        assert_eq!(baked.parts[1].indices.len(), source.parts[1].indices.len());
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

    #[test]
    fn oversized_lightmaps_are_reduced_to_eight_k() {
        let lightmap = lightmap::LightMap {
            pixels: vec![128; (MAX_LIGHTMAP_DIMENSION + 1) * 3],
            width: MAX_LIGHTMAP_DIMENSION + 1,
            height: 1,
        };
        let constrained = constrain_lightmap(lightmap);
        assert_eq!(constrained.width, MAX_LIGHTMAP_DIMENSION);
        assert_eq!(constrained.height, 1);
        assert_eq!(constrained.pixels.len(), MAX_LIGHTMAP_DIMENSION * 3);
    }

    #[test]
    fn final_lightmaps_are_power_of_two_and_capped() {
        for (width, height) in [(694, 694), (700, 700), (1024, 1024), (8193, 1)] {
            let lightmap = lightmap::LightMap {
                pixels: vec![128; width * height * 3],
                width,
                height,
            };
            let prepared = constrain_lightmap(lightmap);

            assert!(prepared.width.is_power_of_two());
            assert!(prepared.height.is_power_of_two());
            assert!(prepared.width <= MAX_LIGHTMAP_DIMENSION);
            assert!(prepared.height <= MAX_LIGHTMAP_DIMENSION);
            assert_eq!(prepared.pixels.len(), prepared.width * prepared.height * 3);
        }
    }

    #[test]
    fn lightmap_resampling_preserves_atlas_corners() {
        let lightmap = lightmap::LightMap {
            pixels: vec![
                255, 0, 0, 128, 128, 0, 0, 255, 0, 128, 0, 128, 128, 128, 128, 0, 128, 128, 0, 0,
                255, 64, 64, 64, 255, 255, 255,
            ],
            width: 3,
            height: 3,
        };

        let prepared = constrain_lightmap(lightmap);

        assert_eq!((prepared.width, prepared.height), (4, 4));
        assert_eq!(&prepared.pixels[0..3], &[255, 0, 0]);
        assert_eq!(&prepared.pixels[9..12], &[0, 255, 0]);
        assert_eq!(&prepared.pixels[36..39], &[0, 0, 255]);
        assert_eq!(&prepared.pixels[45..48], &[255, 255, 255]);
    }

    #[test]
    fn ambient_is_added_without_wrapping() {
        let mut lightmap = lightmap::LightMap {
            pixels: vec![0, 1, 250, 255, 255, 255],
            width: 2,
            height: 1,
        };

        add_ambient(&mut lightmap, [32, 32, 32]);

        assert_eq!(lightmap.pixels, [32, 33, 255, 255, 255, 255]);
    }

    #[test]
    fn dds_input_preserves_atlas_row_order() {
        let lightmap = lightmap::LightMap {
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            width: 2,
            height: 2,
        };

        assert_eq!(
            expand_rgb_to_rgba(&lightmap),
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
    }

    #[test]
    fn dds_writer_emits_bc7() {
        let path =
            std::env::temp_dir().join(format!("morrobroom-lightmap-{}.dds", std::process::id()));
        let lightmap = lightmap::LightMap {
            pixels: (0..64)
                .flat_map(|index| {
                    let red = u8::try_from(index).expect("test pixel index must fit in a byte") * 4;
                    [red, 255 - red, 128]
                })
                .collect(),
            width: 8,
            height: 8,
        };

        write_dds(&path, &lightmap).expect("BC7 lightmap should write");
        let bytes = std::fs::read(&path).expect("written DDS should be readable");
        let dds = Dds::read(bytes.as_slice()).expect("written DDS should parse");
        assert_eq!(
            dds.header10.expect("BC7 requires DXGI header").dxgi_format,
            rusty_dds::DxgiFormat::BC7_UNorm
        );
        std::fs::remove_file(path).expect("temporary DDS should be removable");
    }
}
