use std::collections::HashSet;

use morrobroom::render_mesh::RenderPart;
use morrobroom::slipgate::repr;
use morrobroom::slipgate::{
    Vector2 as SV2, Vector3 as SV3, brush::BrushId, entity::EntityId, face::FaceId,
};
use tes3::nif::{ApplyMode, ClampMode, FilterMode, NiTriShape, NiTriShapeData};

use crate::{Mesh, lightmap_bake::BakedPart, map_data::MapData, surfaces};

macro_rules! define_enum_with_fromstr {
    (
        $(#[$meta:meta])*
            $vis:vis enum $name:ident {
                $(
                    $variant:ident = $value:expr
                ),* $(,)?
            }
        default = $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq)]
            $vis enum $name {
                $(
                    $variant = $value,
                )*
            }

        impl std::str::FromStr for $name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.parse::<i32>() {
                    $(
                        Ok($value) => Ok($name::$variant),
                    )*
                        Ok(v) => {
                            println!("WARNING: Falling through to default value {:?} for {} (received {})",
                                     $name::$default, stringify!($name), v);
                            Ok($name::$default)
                        },
                    Err(_) => Err(format!("Cannot parse '{}' as {}", s, stringify!($name))),
                }
            }
        }

        impl Default for $name {
            fn default() -> $name {
                $name::$default
            }
        }
    }
}

define_enum_with_fromstr! {
    pub enum BrushSourceBlendMode {
        One = 0,
        Zero = 2,
        SourceColor = 4,
        OneMinusSourceColor = 6,
        DestinationColor = 8,
        OneMinusDestinationColor = 10,
        SourceAlpha = 12,
        OneMinusSourceAlpha = 14,
        DestinationAlpha = 16,
        OneMinusDestinationALpha = 18,
        SourceAlphaSaturate = 20,
    }
    default = SourceAlpha
}

define_enum_with_fromstr! {
    pub enum BrushDestinationBlendMode {
        One = 0,
        Zero = 32,
        SourceColor = 64,
        OneMinusSourceColor = 96,
        DestinationColor = 128,
        OneMinusDestinationColor = 160,
        SourceAlpha = 192,
        OneMinusSourceAlpha = 224,
        DestinationAlpha = 256,
        OneMinusDestinationALpha = 288,
        SourceAlphaSaturate = 320,
    }
    default = OneMinusSourceAlpha
}

define_enum_with_fromstr! {
    pub enum BrushAlphaTestFunction {
        Always = 0,
        Less = 1024,
        Equal = 2048,
        LessThanOrEqual = 3072,
        GreaterThan = 4096,
        NotEqual = 5120,
        GreaterThanOrEqual = 6144,
        Never = 7168,
    }
    default = GreaterThan
}

define_enum_with_fromstr! {
    #[allow(clippy::upper_case_acronyms, reason = "These names are part of the NIF material flag vocabulary.")]
    pub enum BrushUseAlpha {
        OFF = 0,
        BlendEnable = 1,
        TestEnable = 512,
    }
    default = TestEnable
}

define_enum_with_fromstr! {
    #[allow(clippy::upper_case_acronyms, reason = "These names are part of the NIF material flag vocabulary.")]
    pub enum BrushNoSort {
        OFF = 0,
        ON = 8192,
    }
    default = OFF
}

#[derive(Debug, Default, PartialEq)]
pub struct BrushNiAlphaProps {
    pub opacity: Option<f32>,
    pub use_blend: Option<BrushUseAlpha>,
    pub blend_source_mode: Option<BrushSourceBlendMode>,
    pub blend_destination_mode: Option<BrushDestinationBlendMode>,
    pub use_test: Option<BrushUseAlpha>,
    pub test_function: Option<BrushAlphaTestFunction>,
    pub test_threshold: Option<u8>,
    pub no_sort: Option<BrushNoSort>,
}

impl BrushNiAlphaProps {
    pub fn to_flags(&self) -> u16 {
        self.use_blend.unwrap_or_default() as u16
            | self.blend_source_mode.unwrap_or_default() as u16
            | self.blend_destination_mode.unwrap_or_default() as u16
            | self.use_test.unwrap_or_default() as u16
            | self.test_function.unwrap_or_default() as u16
            | self.no_sort.unwrap_or_default() as u16
    }
}

#[derive(Default, PartialEq)]
pub struct BrushNiColorProps {
    pub emissive: Option<[f32; 3]>,
    pub ambient: Option<[f32; 3]>,
    pub diffuse: Option<[f32; 3]>,
    pub specular: Option<[f32; 3]>,
}

#[derive(Default, PartialEq)]
pub struct BrushNiTextureProps {
    pub apply_mode: Option<ApplyMode>,
    pub clamp_mode: Option<ClampMode>,
    pub filter_mode: Option<FilterMode>,
    pub dark_map: Option<String>,
    pub detail_map: Option<String>,
    pub gloss_map: Option<String>,
    pub glow_map: Option<String>,
    pub bump_map: Option<String>,
}

#[derive(Default, PartialEq)]
pub struct BrushNiUvProps {
    pub mode: Option<u32>,
    pub u_rate: Option<f32>,
    pub v_rate: Option<f32>,
    pub period: Option<f32>,
}

#[derive(Default, PartialEq)]
pub struct BrushNiMatProps {
    pub color: BrushNiColorProps,
    pub alpha: BrushNiAlphaProps,
    pub glossiness: Option<f32>,
    pub texturing: BrushNiTextureProps,
    pub uv: BrushNiUvProps,
    pub link_name: Option<String>,
    pub target: Option<String>,
}

fn apply_mode(value: &str) -> Option<ApplyMode> {
    match value.parse::<i32>().ok()? {
        0 => Some(ApplyMode::Replace),
        1 => Some(ApplyMode::Decal),
        2 => Some(ApplyMode::Modulate),
        3 => Some(ApplyMode::Hilight),
        4 => Some(ApplyMode::Hilight2),
        _ => None,
    }
}

fn clamp_mode(value: &str) -> Option<ClampMode> {
    match value.parse::<i32>().ok()? {
        0 => Some(ClampMode::ClampSClampT),
        1 => Some(ClampMode::ClampSWrapT),
        2 => Some(ClampMode::WrapSClampT),
        3 => Some(ClampMode::WrapSWrapT),
        _ => None,
    }
}

fn filter_mode(value: &str) -> Option<FilterMode> {
    match value.parse::<i32>().ok()? {
        0 => Some(FilterMode::Nearest),
        1 => Some(FilterMode::Bilerp),
        2 => Some(FilterMode::Trilerp),
        3 => Some(FilterMode::NearestMipNearest),
        4 => Some(FilterMode::NearestMipLerp),
        5 => Some(FilterMode::BilerpMipNearest),
        _ => None,
    }
}

#[derive(Default)]
pub struct BrushNiNode {
    pub vis_shape: NiTriShape,
    pub vis_data: NiTriShapeData,
    pub vis_verts: Vec<SV3>,
    pub use_emissive: bool,
    pub texture: String,
    pub col_shape: NiTriShape,
    pub col_data: NiTriShapeData,
    pub col_verts: Vec<SV3>,
    pub distance_from_origin: SV3,
    // Mesh color values when doing more direct edits
    pub mat_props: BrushNiMatProps,
    // Textures and triangles are only used internally
    normals: Vec<SV3>,
    uv_sets: Vec<SV2>,
    lightmap_uv_sets: Vec<SV2>,
    vis_tris: Vec<Vec<usize>>,
    col_tris: Vec<Vec<usize>>,
}

impl BrushNiNode {
    /// The name of this function might be a bit confusing, as it returns a set of nodes
    /// But one brush may have multiple textures, whereas one `TriShape` should only
    /// ever have one texture. So even though we are requesting information for one brush,
    /// Any one brush might be an arbitrary number of `TriShapes` due to texture splitting.
    pub fn from_brush(
        brush_id: BrushId,
        entity_id: EntityId,
        map_data: &MapData,
    ) -> Vec<BrushNiNode> {
        let mut face_nodes = Vec::new();

        let faces_with_textures = Self::collect_faces_with_textures(brush_id, map_data);

        for face_set in faces_with_textures {
            face_nodes.push(Self::node_from_faces(
                &face_set, map_data, entity_id, brush_id,
            ));
        }

        for node in &mut face_nodes {
            node.collect();
        }

        face_nodes
    }

    /// Given a &str from a parsed map, convert it into a float array
    pub fn get_color(color_str: &str) -> [f32; 3] {
        color_str
            .split_whitespace()
            .take(3)
            .map(|s| s.parse().unwrap_or_default())
            .collect::<Vec<f32>>()
            .try_into()
            .expect("Color props value was invalid!")
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The FGD property is a float for editor convenience, while NiAlphaProperty stores an 8-bit threshold."
    )]
    fn alpha_threshold(value: &str) -> Option<u8> {
        let value = value.parse::<f32>().ok()?;
        if !value.is_finite() || !(0.0..=f32::from(u8::MAX)).contains(&value) {
            return None;
        }
        Some(value.round() as u8)
    }

    // Given a set of faces, expressed by a brush (or brush entity), create a corresponding BrushNiNode
    // BrushNiNodes contain all the relevant data for a NIF, but aren't quite the nif-ready format
    fn node_from_faces(
        faces: &[FaceId],
        map_data: &MapData,
        entity_id: EntityId,
        brush_id: BrushId,
    ) -> BrushNiNode {
        let mut node = BrushNiNode::default();

        let entity_props = map_data.get_entity_properties(entity_id);
        node.mat_props = Self::material_props(&entity_props);

        Self::append_faces(&mut node, faces, map_data, brush_id);
        node
    }

    fn material_props(
        entity_props: &std::collections::HashMap<&String, &String>,
    ) -> BrushNiMatProps {
        let mut props = BrushNiMatProps::default();
        Self::material_color_props(entity_props, &mut props);
        Self::material_alpha_props(entity_props, &mut props);
        Self::texturing_props(entity_props, &mut props);
        Self::uv_props(entity_props, &mut props);
        Self::link_props(entity_props, &mut props);
        props
    }

    fn material_color_props(
        entity_props: &std::collections::HashMap<&String, &String>,
        props: &mut BrushNiMatProps,
    ) {
        for color_type in ["Ambient", "Diffuse", "Emissive", "Specular"] {
            if let Some(color) = entity_props.get(&format!("Material_{color_type}_color")) {
                let color_value = Self::get_color(color);
                match color_type {
                    "Ambient" => props.color.ambient = Some(color_value),
                    "Diffuse" => props.color.diffuse = Some(color_value),
                    "Emissive" => props.color.emissive = Some(color_value),
                    "Specular" => props.color.specular = Some(color_value),
                    _ => unreachable!(),
                }
            }
        }

        props.glossiness = entity_props
            .get(&"Material_Glossiness".to_string())
            .and_then(|value| value.parse::<f32>().ok());
    }

    fn material_alpha_props(
        entity_props: &std::collections::HashMap<&String, &String>,
        props: &mut BrushNiMatProps,
    ) {
        for alpha_prop in [
            "UseBlend",
            "BlendSourceMode",
            "BlendDestinationMode",
            "TestEnable",
            "TestFunction",
            "TestThreshold",
            "NoSort",
        ] {
            if let Some(prop) = entity_props.get(&format!("Material_Alpha_{alpha_prop}")) {
                match alpha_prop {
                    "UseBlend" => props.alpha.use_blend = prop.parse().ok(),
                    "BlendSourceMode" => props.alpha.blend_source_mode = prop.parse().ok(),
                    "BlendDestinationMode" => {
                        props.alpha.blend_destination_mode = prop.parse().ok();
                    }
                    "TestEnable" => props.alpha.use_test = prop.parse().ok(),
                    "TestFunction" => props.alpha.test_function = prop.parse().ok(),
                    "TestThreshold" => props.alpha.test_threshold = Self::alpha_threshold(prop),
                    "NoSort" => props.alpha.no_sort = prop.parse().ok(),
                    _ => unreachable!(),
                }
            }
        }

        if let Some(value) = entity_props.get(&"Material_Alpha".to_string()) {
            props.alpha.opacity = Some(
                value
                    .parse()
                    .expect("Failed to parse float value from material properties!"),
            );
        }
    }

    fn texturing_props(
        entity_props: &std::collections::HashMap<&String, &String>,
        props: &mut BrushNiMatProps,
    ) {
        props.texturing.apply_mode = entity_props
            .get(&"Nif_Texture_ApplyMode".to_string())
            .and_then(|value| apply_mode(value));
        props.texturing.clamp_mode = entity_props
            .get(&"Nif_Texture_ClampMode".to_string())
            .and_then(|value| clamp_mode(value));
        props.texturing.filter_mode = entity_props
            .get(&"Nif_Texture_FilterMode".to_string())
            .and_then(|value| filter_mode(value));
        props.texturing.dark_map = entity_props
            .get(&"Nif_Texture_DarkMap".to_string())
            .map(|value| (*value).clone());
        props.texturing.detail_map = entity_props
            .get(&"Nif_Texture_DetailMap".to_string())
            .map(|value| (*value).clone());
        props.texturing.gloss_map = entity_props
            .get(&"Nif_Texture_GlossMap".to_string())
            .map(|value| (*value).clone());
        props.texturing.glow_map = entity_props
            .get(&"Nif_Texture_GlowMap".to_string())
            .map(|value| (*value).clone());
        props.texturing.bump_map = entity_props
            .get(&"Nif_Texture_BumpMap".to_string())
            .map(|value| (*value).clone());
    }

    fn uv_props(
        entity_props: &std::collections::HashMap<&String, &String>,
        props: &mut BrushNiMatProps,
    ) {
        props.uv.mode = entity_props
            .get(&"Nif_UV_Mode".to_string())
            .and_then(|value| value.parse::<u32>().ok());
        props.uv.u_rate = entity_props
            .get(&"Nif_UV_U".to_string())
            .and_then(|value| value.parse::<f32>().ok());
        props.uv.v_rate = entity_props
            .get(&"Nif_UV_V".to_string())
            .and_then(|value| value.parse::<f32>().ok());
        props.uv.period = entity_props
            .get(&"Nif_UV_Period".to_string())
            .and_then(|value| value.parse::<f32>().ok());
    }

    fn link_props(
        entity_props: &std::collections::HashMap<&String, &String>,
        props: &mut BrushNiMatProps,
    ) {
        props.link_name = entity_props
            .get(&"Nif_LinkName".to_string())
            .map(|value| (*value).clone());
        props.target = entity_props
            .get(&"Nif_Target".to_string())
            .map(|value| (*value).clone());
    }

    fn append_faces(
        node: &mut BrushNiNode,
        faces: &[FaceId],
        map_data: &MapData,
        brush_id: BrushId,
    ) {
        for (part_index, part) in map_data.render_mesh.parts.iter().enumerate() {
            if part.source_brush != brush_id || !faces.contains(&part.source_face) {
                continue;
            }
            let lightmap_part = map_data
                .lightmap_geometry()
                .and_then(|geometry| geometry.part(part_index));
            Self::append_render_part(node, part, lightmap_part);
        }

        for face_id in faces {
            let texture_id = map_data.geomap.face_textures.get(*face_id).unwrap();
            let texture_name = map_data.geomap.textures.get(*texture_id).unwrap();

            if texture_name == "skip" || texture_name.contains("skip_") {
                continue;
            }

            let (_content_flags, mut surface_flags, _value) = match &map_data
                .geomap
                .face_extensions
                .get(*face_id)
                .unwrap_or(&repr::Extension::Standard)
            {
                &repr::Extension::Quake2 {
                    content_flags,
                    surface_flags,
                    value,
                } => (*content_flags, *surface_flags, *value),
                _ => (0, 0, 0.0),
            };

            let vertices = &map_data.face_vertices.get(*face_id).unwrap();

            let indices = if surface_flags & surfaces::NiBroomSurface::InvertFaces as u32 != 0 {
                map_data.inverted_face_tri_indices.get(*face_id).unwrap_or_else(|| {
panic!("Critical error: Missing inverted face triangle indices for face_id: {face_id:?}")
})
            } else {
                map_data.face_tri_indices.get(*face_id).unwrap_or_else(|| {
                    panic!(
                        "Critical error: Missing face triangle indices for face_id: {face_id:?} on brush: {brush_id:?}"
                    )
                })
            };

            // We can't do fuzzier matches on this, so,
            // we'll have to hardcode a set of sky texture names (Thanks skyrim)
            if texture_name.eq_ignore_ascii_case("sky5_blu") {
                node.use_emissive = true;
            }

            // Test for water or slime types
            if texture_name.to_ascii_lowercase().contains("slime")
                || texture_name.to_ascii_lowercase().contains("water")
                || texture_name.to_ascii_lowercase().contains("lava")
                || texture_name.to_ascii_lowercase().contains("mwat")
            {
                surface_flags |= surfaces::NiBroomSurface::NoClip as u32;
                println!("{face_id} interpreted as liquid, added NoClip flag");
            }

            // The node will always have an RCN, only populate it if the NoClip flag is NOT applied to this surface
            if surface_flags & surfaces::NiBroomSurface::NoClip as u32 == 0 {
                node.col_verts.extend(*vertices);
                node.col_tris.push((*indices).clone());
            }
        }
    }

    fn append_render_part(
        node: &mut BrushNiNode,
        part: &RenderPart,
        lightmap_part: Option<&BakedPart>,
    ) {
        node.use_emissive |= part.material.emissive;
        node.texture.clone_from(&part.material.texture);
        if let Some(lightmap_part) = lightmap_part {
            node.normals.extend(
                lightmap_part
                    .vertices
                    .iter()
                    .map(|vertex| vertex.render.normal),
            );
            node.uv_sets
                .extend(lightmap_part.vertices.iter().map(|vertex| vertex.render.uv));
            node.lightmap_uv_sets.extend(
                lightmap_part
                    .vertices
                    .iter()
                    .map(|vertex| vertex.lightmap_uv),
            );
            node.vis_verts.extend(
                lightmap_part
                    .vertices
                    .iter()
                    .map(|vertex| vertex.render.position),
            );
            node.vis_tris.push(
                lightmap_part
                    .indices
                    .iter()
                    .map(|index| *index as usize)
                    .collect(),
            );
            return;
        }
        node.normals
            .extend(part.vertices.iter().map(|vertex| vertex.normal));
        node.uv_sets
            .extend(part.vertices.iter().map(|vertex| vertex.uv));
        node.vis_verts
            .extend(part.vertices.iter().map(|vertex| vertex.position));
        node.vis_tris
            .push(part.indices.iter().map(|index| *index as usize).collect());
    }

    fn collect_faces_with_textures(brush_id: BrushId, map_data: &MapData) -> Vec<Vec<FaceId>> {
        let mut face_textures = Vec::new();

        let faces = map_data.geomap.brush_faces.get(brush_id).unwrap();

        for face in faces {
            let texture_id = map_data.geomap.face_textures.get(*face).unwrap();
            let texture_name = map_data.geomap.textures.get(*texture_id).unwrap();
            if !face_textures.contains(texture_name) {
                face_textures.push(texture_name.clone());
            }
        }

        let mut faces_with_matching_textures: Vec<Vec<FaceId>> =
            vec![Vec::new(); face_textures.len()];

        for (index, texture) in face_textures.iter().enumerate() {
            for face in faces {
                let texture_id = map_data.geomap.face_textures.get(*face).unwrap();
                let texture_name = map_data.geomap.textures.get(*texture_id).unwrap();

                if texture_name == texture {
                    faces_with_matching_textures[index].push(*face);
                }
            }
        }

        faces_with_matching_textures
    }

    /// Renumbers triangles appropriately, then switches the Y and Z coordinates of every vertex
    /// Quake is Y-up, and the triangles are numbered differently (since they're not collected into a mesh like we do)
    fn to_nif_format(shape_data: &mut NiTriShapeData, verts: &[SV3], tris: &[Vec<usize>]) {
        if verts.is_empty() {
            return;
        }

        let mut verts_used = 0;

        for face_tris in tris {
            shape_data
                .triangles
                .extend(face_tris.as_chunks::<3>().0.iter().map(|chunk| {
                    [
                        u16::try_from(chunk[0] + verts_used).expect("NIF vertex index exceeds u16"),
                        u16::try_from(chunk[1] + verts_used).expect("NIF vertex index exceeds u16"),
                        u16::try_from(chunk[2] + verts_used).expect("NIF vertex index exceeds u16"),
                    ]
                }));

            verts_used += face_tris.iter().collect::<HashSet<_>>().len();
        }

        for vert in verts {
            shape_data.vertices.push([vert[0], vert[1], vert[2]].into());
        }
    }

    fn collect(&mut self) {
        if !self.vis_verts.is_empty() {
            self.distance_from_origin = Mesh::centroid(&self.vis_verts);
        }

        Self::to_nif_format(&mut self.vis_data, &self.vis_verts, &self.vis_tris);
        Self::to_nif_format(&mut self.col_data, &self.col_verts, &self.col_tris);

        for normal in &self.normals {
            self.vis_data
                .normals
                .push([normal[0], normal[1], normal[2]].into());
        }

        for uv in &self.uv_sets {
            self.vis_data.uv_sets.push((uv[0], uv[1]).into());
        }
        for uv in &self.lightmap_uv_sets {
            self.vis_data.uv_sets.push((uv[0], uv[1]).into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_render_properties_and_uv_scroll_from_map_properties() {
        let owned = [
            ("Material_Emissive_color", "0.1 0.2 0.3"),
            ("Material_Specular_color", "0.4 0.5 0.6"),
            ("Material_Glossiness", "24"),
            ("Material_Alpha", "0.75"),
            ("Material_Alpha_UseBlend", "1"),
            ("Material_Alpha_BlendSourceMode", "12"),
            ("Material_Alpha_BlendDestinationMode", "224"),
            ("Material_Alpha_TestEnable", "512"),
            ("Material_Alpha_TestFunction", "4096"),
            ("Material_Alpha_TestThreshold", "127.6"),
            ("Material_Alpha_NoSort", "8192"),
            ("Nif_Texture_ApplyMode", "1"),
            ("Nif_Texture_FilterMode", "4"),
            ("Nif_Texture_ClampMode", "1"),
            ("Nif_Texture_DarkMap", "textures/dark.dds"),
            ("Nif_Texture_DetailMap", "textures/detail.dds"),
            ("Nif_Texture_GlossMap", "textures/gloss.dds"),
            ("Nif_Texture_GlowMap", "textures/glow.dds"),
            ("Nif_Texture_BumpMap", "textures/bump.dds"),
            ("Nif_UV_Mode", "1"),
            ("Nif_UV_U", "0.25"),
            ("Nif_UV_V", "-0.5"),
            ("Nif_LinkName", "surface"),
            ("Nif_Target", "billboard"),
        ];
        let owned = owned.map(|(key, value)| (key.to_owned(), value.to_owned()));
        let properties: std::collections::HashMap<_, _> =
            owned.iter().map(|(key, value)| (key, value)).collect();

        let parsed = BrushNiNode::material_props(&properties);
        assert_eq!(parsed.color.emissive, Some([0.1, 0.2, 0.3]));
        assert_eq!(parsed.color.specular, Some([0.4, 0.5, 0.6]));
        assert_eq!(parsed.glossiness, Some(24.0));
        assert_eq!(parsed.alpha.opacity, Some(0.75));
        assert_eq!(parsed.alpha.test_threshold, Some(128));
        assert_eq!(parsed.texturing.apply_mode, Some(ApplyMode::Decal));
        assert_eq!(
            parsed.texturing.filter_mode,
            Some(FilterMode::NearestMipLerp)
        );
        assert_eq!(parsed.texturing.clamp_mode, Some(ClampMode::ClampSWrapT));
        assert_eq!(
            parsed.texturing.dark_map.as_deref(),
            Some("textures/dark.dds")
        );
        assert_eq!(
            parsed.texturing.detail_map.as_deref(),
            Some("textures/detail.dds")
        );
        assert_eq!(
            parsed.texturing.gloss_map.as_deref(),
            Some("textures/gloss.dds")
        );
        assert_eq!(
            parsed.texturing.glow_map.as_deref(),
            Some("textures/glow.dds")
        );
        assert_eq!(
            parsed.texturing.bump_map.as_deref(),
            Some("textures/bump.dds")
        );
        assert_eq!(parsed.uv.mode, Some(1));
        assert_eq!(parsed.uv.u_rate, Some(0.25));
        assert_eq!(parsed.uv.v_rate, Some(-0.5));
        assert_eq!(parsed.link_name.as_deref(), Some("surface"));
        assert_eq!(parsed.target.as_deref(), Some("billboard"));
    }

    #[test]
    fn rejects_alpha_thresholds_outside_the_nif_byte_range() {
        assert_eq!(BrushNiNode::alpha_threshold("255"), Some(u8::MAX));
        assert_eq!(BrushNiNode::alpha_threshold("-1"), None);
        assert_eq!(BrushNiNode::alpha_threshold("256"), None);
        assert_eq!(BrushNiNode::alpha_threshold("NaN"), None);
    }
}
