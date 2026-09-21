use imagesize::size;
use morrobroom::slipgate::repr::*;
use morrobroom::slipgate::{
    GeoMap, Textures,
    entity::EntityId,
    face::{FaceNormals, FaceTriangleIndices, FaceUvs, FaceVertices},
    map_geometry::MapGeometry,
};
use openmw_cfg::{Ini, find_file, get_config};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
};

use crate::Mesh;

const GRID_SIZE: u8 = 128;

pub struct MapData {
    pub geomap: GeoMap,
    #[allow(dead_code)] // Retained for future spatial face queries and broad-phase work.
    pub face_grid: HashMap<[i32; 3], Vec<morrobroom::slipgate::face::FaceId>>,
    pub face_vertices: FaceVertices,
    pub face_tri_indices: FaceTriangleIndices,
    pub inverted_face_tri_indices: FaceTriangleIndices,
    pub flat_normals: FaceNormals,
    pub smooth_normals: FaceNormals,
    pub face_uvs: FaceUvs,
}

impl MapData {
    pub fn new(map_name: &String) -> Self {
        // First load the map from the filesystem and parse it using Slipgate.
        let map = fs::read_to_string(map_name)
            .expect("Reading file failed. Bad news! Does it exist?")
            .parse::<Map>()
            .expect("Map parsing failed!");

        // MapData needs the core geometry, but not the expensive occlusion
        // scans. Keep construction in MapGeometry so there is one dataflow.
        let geometry = MapGeometry::from_map_without_occlusion(map);

        let texture_names = MapData::collect_textures(&geometry.geomap.textures);
        let texture_paths = MapData::find_textures_in_vfs(&texture_names);

        let texture_sizes: BTreeMap<&str, (u32, u32)> = texture_paths
            .iter()
            .map(|texture_name| {
                let texture_size = size(texture_name.clone()).expect(&format!(
                    "Image Processing failed! Is there an issue with the path? {}",
                    texture_name
                ));
                println!(
                    "Mapping texture {0} with sizes: {1}, {2}",
                    texture_name, texture_size.width, texture_size.height
                );
                (
                    texture_name.as_str(),
                    (texture_size.width as u32, texture_size.height as u32),
                )
            })
            .collect();

        let mut modified_textures: Vec<String> = geometry.geomap.textures.iter().cloned().collect();

        for (texture_id, texture_name) in geometry.geomap.textures.iter().enumerate() {
            for texture_path in &texture_paths {
                if texture_path
                    .to_ascii_lowercase()
                    .contains(&texture_name.to_ascii_lowercase())
                {
                    modified_textures[texture_id] = texture_path.to_string();
                }
            }
        }

        let mut textures_with_paths: Textures = Textures::default();
        textures_with_paths.data = modified_textures.into();

        let face_uvs = geometry.face_uvs(morrobroom::slipgate::texture::texture_sizes(
            &textures_with_paths,
            texture_sizes,
        ));

        let face_grid: HashMap<[i32; 3], Vec<morrobroom::slipgate::face::FaceId>> = geometry
            .geomap
            .brush_faces
            .iter()
            .flat_map(|brush_faces| {
                brush_faces.iter().map(|face_id| {
                    let centroid = Mesh::centroid(
                        geometry
                            .face_vertices
                            .get(*face_id)
                            .expect("Face vertices should always be valid"),
                    );
                    let grid_position = [
                        (centroid.x.round() / GRID_SIZE as f32).floor() as i32,
                        (centroid.y.round() / GRID_SIZE as f32).floor() as i32,
                        (centroid.z.round() / GRID_SIZE as f32).floor() as i32,
                    ];
                    (grid_position, *face_id)
                })
            })
            .fold(HashMap::new(), |mut acc, (grid_pos, face_id)| {
                acc.entry(grid_pos).or_insert_with(Vec::new).push(face_id);
                acc
            });

        MapData {
            geomap: geometry.geomap,
            face_grid,
            face_vertices: geometry.face_vertices,
            face_tri_indices: geometry.face_tri_indices,
            inverted_face_tri_indices: geometry.inverted_face_tri_indices,
            flat_normals: geometry.flat_normals,
            smooth_normals: geometry.smooth_normals,
            face_uvs,
        }
    }

    pub fn collect_textures(textures: &Textures) -> HashSet<&String> {
        textures.iter().map(|texture_name| texture_name).collect()
    }

    pub fn find_vfs_texture(name: &str, config: &Ini) -> Option<String> {
        let extensions = ["dds", "tga", "png"];

        extensions
         .iter()
         .find_map(|extension| {
             let full_name = format!("Textures/{}.{}", name, extension);
             println!("Searching for texture: {}", full_name);
             match find_file(config, full_name.as_str()) {
                 std::result::Result::Ok(path) => Some(path.to_string_lossy().to_string()),
                 Err(_) => { None }
             }
         })
         .or_else(|| {
             eprintln!("ERROR: Texture not found! This map is using a texture which isn't in your OpenMW VFS: {}.[dds/tga/png]", name);
             None
         })
    }

    pub fn find_textures_in_vfs(textures: &HashSet<&String>) -> HashSet<String> {
        let config = get_config().expect("Openmw.cfg not detected! Please ensure you have a valid openmw configuration file in the canonical system directory.");
        textures
            .iter()
            .filter_map(|texture_name| MapData::find_vfs_texture(&texture_name, &config))
            .collect()
    }

    pub fn get_entity_properties(&self, entity_id: &EntityId) -> HashMap<&String, &String> {
        let entity_properties = self.geomap.entity_properties.get(*entity_id);

        // Group names are powers of 2 and have different keys in the group definition and separate entities which reference it
        if let None = entity_properties {
            panic!("brush entity {} has no properties!", entity_id);
        }

        entity_properties
            .unwrap()
            .iter()
            .fold(HashMap::new(), |mut acc, prop| {
                acc.insert(&prop.key, &prop.value);
                acc
            })
    }
}

// pub use crate::map_data::MapData;
