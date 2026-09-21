use imagesize::blob_size;
use morrobroom::render_mesh::RenderMesh;
use morrobroom::slipgate::csg::GeometryTolerance;
use morrobroom::slipgate::repr::Map;
use morrobroom::slipgate::{
    GeoMap, Textures,
    entity::EntityId,
    face::{FaceTriangleIndices, FaceVertices},
    map_geometry::MapGeometry,
};
use openmw_config::OpenMWConfiguration;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::Read,
};
use vfstool_lib::VFS;

use crate::Mesh;

const GRID_SIZE: u8 = 128;

#[allow(
    clippy::cast_possible_truncation,
    reason = "The spatial index is an i32 grid; coordinates outside its range are saturated by the Rust cast contract."
)]
fn grid_coordinate(coordinate: f32) -> i32 {
    (coordinate.round() / f32::from(GRID_SIZE)).floor() as i32
}

pub struct MapData {
    pub geomap: GeoMap,
    #[allow(dead_code)] // Retained for future spatial face queries and broad-phase work.
    pub face_grid: HashMap<[i32; 3], Vec<morrobroom::slipgate::face::FaceId>>,
    pub face_vertices: FaceVertices,
    pub face_tri_indices: FaceTriangleIndices,
    pub inverted_face_tri_indices: FaceTriangleIndices,
    pub render_mesh: RenderMesh,
    vfs: VFS,
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

        let openmw_config = OpenMWConfiguration::from_env()
            .expect("Openmw.cfg not detected! Please ensure you have a valid OpenMW configuration file in the canonical system directory.");
        let fallback_archives: Vec<&str> = openmw_config
            .fallback_archives_iter()
            .map(|archive| archive.value().as_str())
            .collect();
        let vfs = VFS::from_directories(
            openmw_config
                .data_directories_iter()
                .map(openmw_config::DirectorySetting::parsed),
            Some(fallback_archives),
        );

        let texture_names = MapData::collect_textures(&geometry.geomap.textures);
        let texture_sizes: BTreeMap<&str, (u32, u32)> = texture_names
            .iter()
            .filter_map(|texture_name| {
                MapData::find_vfs_texture(texture_name, &vfs).map(|(path, dimensions)| {
                    println!(
                        "Mapping texture {0} with sizes: {1}, {2}",
                        path, dimensions.0, dimensions.1
                    );
                    (texture_name.as_str(), dimensions)
                })
            })
            .collect();

        let texture_sizes =
            morrobroom::slipgate::texture::texture_sizes(&geometry.geomap.textures, &texture_sizes);
        let render_mesh =
            RenderMesh::from_geometry(&geometry, &texture_sizes, GeometryTolerance::default())
                .expect("map faces should compile into render geometry");

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
                        grid_coordinate(centroid.x),
                        grid_coordinate(centroid.y),
                        grid_coordinate(centroid.z),
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
            render_mesh,
            vfs,
        }
    }

    pub fn collect_textures(textures: &Textures) -> HashSet<&String> {
        textures.iter().collect()
    }

    pub fn find_vfs_texture(name: &str, vfs: &VFS) -> Option<(String, (u32, u32))> {
        let extensions = ["dds", "tga", "png"];

        extensions
         .iter()
         .find_map(|extension| {
              let full_name = format!("Textures/{name}.{extension}");
              println!("Searching for texture: {full_name}");
              let file = vfs.get_file(full_name.as_str())?;
              let mut reader = file.open().ok()?;
              let mut bytes = Vec::new();
              reader.read_to_end(&mut bytes).ok()?;
              let image_size = blob_size(&bytes).ok()?;
               Some((
                   full_name,
                   (
                       u32::try_from(image_size.width).expect("texture width exceeds u32"),
                       u32::try_from(image_size.height).expect("texture height exceeds u32"),
                   ),
               ))
          })
          .or_else(|| {
              eprintln!("ERROR: Texture not found! This map is using a texture which isn't in your OpenMW VFS: {name}.[dds/tga/png]");
              None
          })
    }

    pub fn vfs(&self) -> &VFS {
        &self.vfs
    }

    pub fn get_entity_properties(&self, entity_id: EntityId) -> HashMap<&String, &String> {
        let entity_properties = self.geomap.entity_properties.get(entity_id);

        // Group names are powers of 2 and have different keys in the group definition and separate entities which reference it
        assert!(
            entity_properties.is_some(),
            "brush entity {entity_id} has no properties!"
        );

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
