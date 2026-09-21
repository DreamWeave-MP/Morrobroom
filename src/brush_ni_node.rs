use std::collections::HashSet;

use morrobroom::slipgate::repr;
use morrobroom::slipgate::{
    Vector2 as SV2, Vector3 as SV3, brush::BrushId, entity::EntityId, face::FaceId,
};
use tes3::nif::{NiTriShape, NiTriShapeData};

use crate::{Mesh, map_data::MapData, surfaces};

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
    pub enum BrushUseAlpha {
        OFF = 0,
        BlendEnable = 1,
        TestEnable = 512,
    }
    default = TestEnable
}

define_enum_with_fromstr! {
    pub enum BrushNoSort {
        OFF = 0,
        ON = 8196,
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
}

#[derive(PartialEq)]
pub struct BrushNiMatProps {
    pub color: BrushNiColorProps,
    pub alpha: BrushNiAlphaProps,
}

impl BrushNiMatProps {
    pub fn default() -> BrushNiMatProps {
        BrushNiMatProps {
            color: BrushNiColorProps::default(),
            alpha: BrushNiAlphaProps::default(),
        }
    }
}

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
    vis_tris: Vec<Vec<usize>>,
    col_tris: Vec<Vec<usize>>,
}

impl BrushNiNode {
    pub fn from_brushes(
        brushes: &[BrushId],
        map_data: &MapData,
        entity_id: &EntityId,
    ) -> Vec<BrushNiNode> {
        brushes
            .iter()
            .flat_map(|brush_id| BrushNiNode::from_brush(brush_id, entity_id, map_data))
            .collect()
    }

    /// The name of this function might be a bit confusing, as it returns a set of nodes
    /// But one brush may have multiple textures, whereas one TriShape should only
    /// ever have one texture. So even though we are requesting information for one brush,
    /// Any one brush might be an arbitrary number of TriShapes due to texture splitting.
    pub fn from_brush(
        brush_id: &BrushId,
        entity_id: &EntityId,
        map_data: &MapData,
    ) -> Vec<BrushNiNode> {
        let mut face_nodes = Vec::new();

        let faces_with_textures = Self::collect_faces_with_textures(&brush_id, map_data);

        for face_set in faces_with_textures {
            face_nodes.push(Self::node_from_faces(
                &face_set, &map_data, entity_id, brush_id,
            ));
        }

        for node in &mut face_nodes {
            node.collect()
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

    // Given a set of faces, expressed by a brush (or brush entity), create a corresponding BrushNiNode
    // BrushNiNodes contain all the relevant data for a NIF, but aren't quite the nif-ready format
    fn node_from_faces(
        faces: &Vec<FaceId>,
        map_data: &MapData,
        entity_id: &EntityId,
        brush_id: &BrushId,
    ) -> BrushNiNode {
        let mut node = BrushNiNode::default();

        let entity_props = map_data.get_entity_properties(entity_id);

        ["Ambient", "Diffuse", "Emissive"]
            .iter()
            .for_each(|color_type| {
                if let Some(color) = entity_props.get(&format!("Material_{}_color", color_type)) {
                    let color_value = Some(Self::get_color(color));
                    match *color_type {
                        "Ambient" => node.mat_props.color.ambient = color_value,
                        "Diffuse" => node.mat_props.color.diffuse = color_value,
                        "Emissive" => node.mat_props.color.emissive = color_value,
                        _ => unreachable!(),
                    }
                }
            });

        [
            "UseBlend",
            "BlendSourceMode",
            "BlendDestinationMode",
            "TestEnable",
            "TestFunction",
            "TestThreshold",
            "NoSort",
        ]
        .iter()
        .for_each(|alpha_prop| {
            if let Some(prop) = entity_props.get(&format!("Material_Alpha_{}", alpha_prop)) {
                match *alpha_prop {
                    "UseBlend" => {
                        if let Ok(value) = prop.parse::<BrushUseAlpha>() {
                            node.mat_props.alpha.use_blend = Some(value);
                        }
                    }
                    "BlendSourceMode" => {
                        if let Ok(value) = prop.parse::<BrushSourceBlendMode>() {
                            node.mat_props.alpha.blend_source_mode = Some(value);
                        }
                    }
                    "BlendDestinationMode" => {
                        if let Ok(value) = prop.parse::<BrushDestinationBlendMode>() {
                            node.mat_props.alpha.blend_destination_mode = Some(value);
                        }
                    }
                    "TestEnable" => {
                        if let Ok(value) = prop.parse::<BrushUseAlpha>() {
                            node.mat_props.alpha.use_test = Some(value);
                        }
                    }
                    "TestFunction" => {
                        if let Ok(value) = prop.parse::<BrushAlphaTestFunction>() {
                            node.mat_props.alpha.test_function = Some(value);
                        }
                    }
                    "TestThreshold" => {
                        if let Ok(value) = prop.parse::<u8>() {
                            node.mat_props.alpha.test_threshold = Some(value);
                        }
                    }
                    "NoSort" => {
                        if let Ok(value) = prop.parse::<BrushNoSort>() {
                            node.mat_props.alpha.no_sort = Some(value);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        });

        if let Some(value) = entity_props.get(&"Material_Alpha".to_string()) {
            node.mat_props.alpha.opacity = Some(
                value
                    .parse()
                    .expect("Failed to parse float value from material properties!"),
            );
        }

        for face_id in faces.iter() {
            let texture_id = map_data.geomap.face_textures.get(*face_id).unwrap();
            let texture_name = map_data.geomap.textures.get(texture_id).unwrap();

            if texture_name == "skip" || texture_name.contains("skip_") {
                continue;
            };

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
panic!("Critical error: Missing inverted face triangle indices for face_id: {:?}", face_id)
})
            } else {
                map_data.face_tri_indices.get(*face_id).unwrap_or_else(|| {
                    panic!(
                        "Critical error: Missing face triangle indices for face_id: {:?} on brush: {:?}",
                        face_id,
                        brush_id
                    )
                })
            };

            // We can't do fuzzier matches on this, so,
            // we'll have to hardcode a set of sky texture names (Thanks skyrim)
            if texture_name.to_ascii_lowercase() == "sky5_blu" {
                node.use_emissive = true;
            }

            // Test for water or slime types
            if texture_name.to_ascii_lowercase().contains("slime")
                || texture_name.to_ascii_lowercase().contains("water")
                || texture_name.to_ascii_lowercase().contains("lava")
                || texture_name.to_ascii_lowercase().contains("mwat")
            {
                surface_flags |= surfaces::NiBroomSurface::NoClip as u32;
                println!("{face_id} interpreted as liquid, added NoClip flag")
            }

            // Get Texture uvs for this specific face out of parsed map data
            let uv_sets = &map_data
                .face_uvs
                .get(*face_id)
                .expect("Unable to collect face UVs for {face_id}");

            // For any texture that isn't a `clip`, apply the calculated normals, verts, tris, uv sets, and texture
            if texture_name != "clip" {
                node.normals.extend(
                    if surface_flags & surfaces::NiBroomSurface::SmoothShading as u32 == 0 {
                        &*map_data.flat_normals.get(*face_id).unwrap()
                    } else {
                        &*map_data.smooth_normals.get(*face_id).unwrap()
                    },
                );
                node.uv_sets.extend(*uv_sets);

                node.vis_verts.extend(*vertices);
                node.vis_tris.push((*indices).to_vec());
                node.texture = texture_name.to_string();
            }

            // The node will always have an RCN, only populate it if the NoClip flag is NOT applied to this surface
            if surface_flags & surfaces::NiBroomSurface::NoClip as u32 == 0 {
                node.col_verts.extend(*vertices);
                node.col_tris.push((*indices).to_vec());
            }
        }
        node
    }

    fn collect_faces_with_textures(brush_id: &BrushId, map_data: &MapData) -> Vec<Vec<FaceId>> {
        let mut face_textures = Vec::new();

        let faces = map_data.geomap.brush_faces.get(brush_id).unwrap();

        for face in faces.iter() {
            let texture_id = map_data.geomap.face_textures.get(*face).unwrap();
            let texture_name = map_data.geomap.textures.get(texture_id).unwrap();
            if !face_textures.contains(texture_name) {
                face_textures.push(texture_name.to_string())
            }
        }

        let mut faces_with_matching_textures: Vec<Vec<FaceId>> =
            vec![Vec::new(); face_textures.len()];

        for (index, texture) in face_textures.iter().enumerate() {
            for face in faces.iter() {
                let texture_id = map_data.geomap.face_textures.get(*face).unwrap();
                let texture_name = map_data.geomap.textures.get(texture_id).unwrap();

                if texture_name == texture {
                    faces_with_matching_textures[index].push(*face);
                }
            }
        }

        faces_with_matching_textures
    }

    /// Renumbers triangles appropriately, then switches the Y and Z coordinates of every vertex
    /// Quake is Y-up, and the triangles are numbered differently (since they're not collected into a mesh like we do)
    fn to_nif_format(
        shape_data: &mut NiTriShapeData,
        verts: &mut Vec<SV3>,
        tris: &mut Vec<Vec<usize>>,
    ) {
        if verts.is_empty() {
            return;
        };

        let mut verts_used = 0;

        tris.into_iter().for_each(|face_tris| {
            shape_data
                .triangles
                .extend(face_tris.chunks_exact(3).map(|chunk| {
                    [
                        (chunk[0] + verts_used) as u16,
                        (chunk[1] + verts_used) as u16,
                        (chunk[2] + verts_used) as u16,
                    ]
                }));

            verts_used += face_tris.into_iter().collect::<HashSet<_>>().len();
        });

        verts.into_iter().for_each(|vert| {
            shape_data
                .vertices
                .push([vert[0] as f32, vert[1] as f32, vert[2] as f32].into())
        });
    }

    fn collect(&mut self) {
        if self.vis_verts.len() > 0 {
            self.distance_from_origin = Mesh::centroid(&self.vis_verts)
        }

        Self::to_nif_format(&mut self.vis_data, &mut self.vis_verts, &mut self.vis_tris);
        Self::to_nif_format(&mut self.col_data, &mut self.col_verts, &mut self.col_tris);

        for normal in &self.normals {
            self.vis_data
                .normals
                .push([normal[0] as f32, normal[1] as f32, normal[2] as f32].into());
        }

        for uv in &self.uv_sets {
            self.vis_data.uv_sets.push((uv[0], uv[1]).into());
        }
    }
}

impl Default for BrushNiNode {
    fn default() -> BrushNiNode {
        BrushNiNode {
            vis_shape: NiTriShape::default(),
            vis_data: NiTriShapeData::default(),
            vis_verts: Vec::new(),
            vis_tris: Vec::new(),
            use_emissive: false,
            normals: Vec::new(),
            texture: String::new(),
            uv_sets: Vec::new(),
            col_shape: NiTriShape::default(),
            col_data: NiTriShapeData::default(),
            col_verts: Vec::new(),
            col_tris: Vec::new(),
            distance_from_origin: SV3::default(),
            mat_props: BrushNiMatProps::default(),
        }
    }
}
