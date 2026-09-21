use std::collections::BTreeMap;

use crate::slipgate::repr::{
    Brush, BrushPlane, Entity, Extension, Properties, TextureOffset, TrianglePlane,
};
use usage::Usage;

use crate::slipgate::{
    DenseStorage, Vector2, assert_contiguous_ids, brush::BrushId, entity::EntityId, face::FaceId,
    texture::TextureId,
};

pub enum EntitiesTag {}
pub enum BrushesTag {}
pub enum FacesTag {}
pub enum PointEntitiesTag {}
pub enum EntityPropertiesTag {}
pub enum EntityBrushesTag {}
pub enum BrushFacesTag {}
pub enum FaceTrianglePlanesTag {}
pub enum FaceTexturesTag {}
pub enum FaceOffsetsTag {}
pub enum FaceAnglesTag {}
pub enum FaceScalesTag {}
pub enum FaceExtensionsTag {}
pub enum TexturesTag {}

pub type Entities = Usage<EntitiesTag, Vec<EntityId>>;
pub type Brushes = Usage<BrushesTag, Vec<BrushId>>;
pub type Faces = Usage<FacesTag, Vec<FaceId>>;

pub type PointEntities = Usage<PointEntitiesTag, Vec<EntityId>>;

pub type EntityProperties = Usage<EntityPropertiesTag, BTreeMap<EntityId, Properties>>;
pub type EntityBrushes = Usage<EntityBrushesTag, BTreeMap<EntityId, Vec<BrushId>>>;

pub type BrushFaces = Usage<BrushFacesTag, DenseStorage<BrushId, Vec<FaceId>>>;

pub type FaceTrianglePlanes = Usage<FaceTrianglePlanesTag, DenseStorage<FaceId, TrianglePlane>>;
pub type FaceTextures = Usage<FaceTexturesTag, DenseStorage<FaceId, TextureId>>;
pub type FaceOffsets = Usage<FaceOffsetsTag, DenseStorage<FaceId, TextureOffset>>;
pub type FaceAngles = Usage<FaceAnglesTag, DenseStorage<FaceId, f32>>;
pub type FaceScales = Usage<FaceScalesTag, DenseStorage<FaceId, Vector2>>;
pub type FaceExtensions = Usage<FaceExtensionsTag, DenseStorage<FaceId, Extension>>;

pub type Textures = Usage<TexturesTag, DenseStorage<TextureId, String>>;

/// Struct-of-arrays representation of a parsed map.
#[derive(Debug, Default, Clone)]
pub struct GeoMap {
    pub entities: Entities,
    pub brushes: Brushes,
    pub faces: Faces,

    pub textures: Textures,

    pub entity_properties: EntityProperties,
    pub entity_brushes: EntityBrushes,
    pub point_entities: PointEntities,

    pub brush_faces: BrushFaces,

    pub face_planes: FaceTrianglePlanes,
    pub face_textures: FaceTextures,
    pub face_offsets: FaceOffsets,
    pub face_angles: FaceAngles,
    pub face_scales: FaceScales,
    pub face_extensions: FaceExtensions,
}

impl GeoMap {
    pub fn new(crate::slipgate::repr::Map(map): crate::slipgate::repr::Map) -> Self {
        let mut brush_head = 0;
        let mut plane_head = 0;
        let mut texture_head = 0;

        let mut entities = Entities::default();
        let mut brushes = Brushes::default();
        let mut faces = Faces::default();

        let mut entity_properties = EntityProperties::default();
        let mut entity_brushes = EntityBrushes::default();

        let mut brush_faces = Vec::new();

        let mut face_planes = Vec::new();
        let mut face_textures = Vec::new();
        let mut face_offsets = Vec::new();
        let mut face_angles = Vec::new();
        let mut face_scales = Vec::new();
        let mut face_extensions = Vec::new();

        let mut textures = BTreeMap::<String, TextureId>::new();
        let mut texture_names = Vec::new();

        for (
            entity_head,
            Entity {
                properties,
                brushes: crate::slipgate::repr::Brushes(bs),
            },
        ) in map.into_iter().enumerate()
        {
            let entity_id = EntityId(entity_head);

            entities.push(entity_id);
            entity_properties.insert(entity_id, properties);

            for Brush(ps) in bs {
                let brush_id = BrushId(brush_head);
                brush_head += 1;

                brushes.push(brush_id);
                brush_faces.push(Vec::new());
                entity_brushes.entry(entity_id).or_default().push(brush_id);

                for BrushPlane {
                    plane,
                    texture,
                    texture_offset,
                    angle,
                    scale_x,
                    scale_y,
                    extension,
                } in ps
                {
                    let plane_id = FaceId(plane_head);
                    plane_head += 1;

                    faces.push(plane_id);
                    face_planes.push(plane);

                    let texture_id = if let Some(texture_id) = textures.get(&texture) {
                        *texture_id
                    } else {
                        let texture_id = TextureId(texture_head);
                        texture_names.push(texture.clone());
                        textures.insert(texture, texture_id);
                        texture_head += 1;
                        texture_id
                    };

                    face_textures.push(texture_id);

                    face_offsets.push(texture_offset);
                    face_angles.push(angle);
                    face_scales.push(nalgebra::vector![scale_x, scale_y]);
                    face_extensions.push(extension);
                    brush_faces[brush_id.0].push(plane_id);
                }
            }
        }

        let point_entities = entities
            .iter()
            .filter(|entity_id| !entity_brushes.contains_key(entity_id))
            .copied()
            .collect();

        assert_contiguous_ids(brushes.iter().copied());
        assert_contiguous_ids(faces.iter().copied());

        GeoMap {
            entities,
            brushes,
            faces,
            textures: DenseStorage::from_vec(texture_names).into(),
            entity_properties,
            entity_brushes,
            point_entities,
            brush_faces: DenseStorage::from_vec(brush_faces).into(),
            face_planes: DenseStorage::from_vec(face_planes).into(),
            face_textures: DenseStorage::from_vec(face_textures).into(),
            face_offsets: DenseStorage::from_vec(face_offsets).into(),
            face_angles: DenseStorage::from_vec(face_angles).into(),
            face_scales: DenseStorage::from_vec(face_scales).into(),
            face_extensions: DenseStorage::from_vec(face_extensions).into(),
        }
    }
}

impl From<crate::slipgate::repr::Map> for GeoMap {
    fn from(map: crate::slipgate::repr::Map) -> Self {
        GeoMap::new(map)
    }
}
