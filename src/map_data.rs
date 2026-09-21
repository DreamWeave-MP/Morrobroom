use imagesize::blob_size;
use lightmap::{LightMap, light::LightDefinition};
use morrobroom::lightmap_bake::BakedRenderMesh;
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
    /// UV-prepared geometry for the optional lightmap pipeline.
    pub lightmap_geometry: Option<BakedRenderMesh>,
    lightmap: Option<LightMap>,
    lightmap_texture_name: Option<String>,
    vfs: VFS,
}

impl MapData {
    pub fn new(map_name: &String, lightmaps_enabled: bool) -> Self {
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
        let lights = collect_point_lights(&geometry.geomap);
        let ambient = collect_ambient_color(&geometry.geomap);
        let lightmap_geometry = lightmaps_enabled.then(|| {
            BakedRenderMesh::from_render_mesh(&render_mesh, 0.005)
                .expect("render mesh should support lightmap UV generation")
        });
        let (lightmap, lightmap_texture_name) = lightmap_geometry
            .as_ref()
            .and_then(BakedRenderMesh::input_mesh)
            .map_or((None, None), |mesh| {
                let other_meshes = vec![mesh];
                let mut baked = LightMap::new(&other_meshes[0], &other_meshes, &lights, 1);
                // A map without authored point lights should retain its textured
                // appearance rather than becoming black when lightmapping is enabled.
                if lights.is_empty() {
                    baked.pixels.fill(u8::MAX);
                } else {
                    morrobroom::lightmap_bake::add_ambient(&mut baked, ambient);
                }
                let baked = morrobroom::lightmap_bake::constrain_lightmap(baked);
                (
                    Some(baked),
                    Some(format!("{}/lightmap.dds", map_stem(map_name))),
                )
            });

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
            lightmap_geometry,
            lightmap,
            lightmap_texture_name,
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

    #[must_use]
    pub const fn lightmap_geometry(&self) -> Option<&BakedRenderMesh> {
        self.lightmap_geometry.as_ref()
    }

    #[must_use]
    pub const fn lightmap(&self) -> Option<&LightMap> {
        self.lightmap.as_ref()
    }

    #[must_use]
    pub fn lightmap_texture_name(&self) -> Option<&str> {
        self.lightmap_texture_name.as_deref()
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

fn map_stem(map_name: &str) -> &str {
    std::path::Path::new(map_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("lightmap")
}

fn collect_point_lights(geomap: &GeoMap) -> Vec<LightDefinition> {
    geomap
        .point_entities
        .iter()
        .filter_map(|entity_id| {
            let properties = geomap.entity_properties.get(*entity_id)?;
            let classname = property(properties, "classname")?;
            if !classname.contains("Light_Point") {
                return None;
            }
            let radius = classname
                .chars()
                .skip_while(|character| !character.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<f32>()
                .ok()?;
            let position = parse_vector3(property(properties, "origin")?)?;
            let color = parse_color(property(properties, "light_color").unwrap_or("255 255 255"));
            Some(LightDefinition::Point(
                lightmap::light::PointLightDefinition {
                    intensity: 1.0,
                    color,
                    radius,
                    position,
                    sqr_radius: radius * radius,
                },
            ))
        })
        .collect()
}

fn collect_ambient_color(geomap: &GeoMap) -> [u8; 3] {
    geomap
        .entities
        .iter()
        .find_map(|entity_id| {
            let properties = geomap.entity_properties.get(*entity_id)?;
            (property(properties, "classname") == Some("worldspawn")).then(|| {
                parse_byte_color(property(properties, "Ambient_color").unwrap_or("15 15 15"))
            })
        })
        .unwrap_or([15, 15, 15])
}

fn property<'a>(
    properties: &'a morrobroom::slipgate::repr::Properties,
    name: &str,
) -> Option<&'a str> {
    properties
        .iter()
        .find(|property| property.key == name)
        .map(|property| property.value.as_str())
}

fn parse_vector3(value: &str) -> Option<nalgebra::Vector3<f32>> {
    let values = value
        .split_whitespace()
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(nalgebra::Vector3::new(
        *values.first()?,
        *values.get(1)?,
        *values.get(2)?,
    ))
}

fn parse_color(value: &str) -> nalgebra::Vector3<f32> {
    let values = value
        .split_whitespace()
        .map(|component| component.parse::<f32>().unwrap_or(255.0) / 255.0)
        .collect::<Vec<_>>();
    nalgebra::Vector3::new(
        values.first().copied().unwrap_or(1.0),
        values.get(1).copied().unwrap_or(1.0),
        values.get(2).copied().unwrap_or(1.0),
    )
}

fn parse_byte_color(value: &str) -> [u8; 3] {
    let mut color = [15; 3];
    for (channel, component) in value.split_whitespace().take(3).enumerate() {
        if let Ok(value) = component.parse() {
            color[channel] = value;
        }
    }
    color
}

// pub use crate::map_data::MapData;
