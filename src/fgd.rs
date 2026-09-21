use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use openmw_config::{ConfigError, OpenMWConfiguration};
use rayon::prelude::*;
use tes3::esp::{EditorId, Plugin, TES3Object, TypeInfo};
use vfstool_lib::VFS;

mod brush_class_props;
/// To add a new record type to the serializer:
/// 1: Implement the `ToFGDProp` trait for that record type
/// 2: In src/serialize.rs, add the relevant fields to the `serialize_typed_objects_as_fgd` function
/// 3: Add that specific record type, to `serialize_object_as_fgd`, for the serialization of each individual record
/// 4: Add a test module for serializing that specific record type. Just copy and paste one of the existing ones at the bottom of this file and change the tag and the output file name.
/// 5: Add relevant bounds handling to `get_object_bounds_from_nif` and `get_object_model_path`
/// 6: Actually deserialize that record type during loading, in `ConfigurationManager::collect_merged_objects`
mod serialize;

#[derive(Debug)]
pub enum ConfigManagerError {
    OpenMWConfigError(ConfigError),
    MissingPluginErr(String),
    InvalidObjectScale(f32),
}

impl std::fmt::Display for ConfigManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenMWConfigError(err) => write!(f, "Failed reading openmw.cfg chain: {err}"),
            Self::MissingPluginErr(plugin) => write!(f, "Failed to find plugin: {plugin}"),
            Self::InvalidObjectScale(scale) => {
                write!(f, "object scale must be finite and positive, got {scale}")
            }
        }
    }
}

impl std::error::Error for ConfigManagerError {}

impl From<ConfigError> for ConfigManagerError {
    fn from(err: ConfigError) -> ConfigManagerError {
        Self::OpenMWConfigError(err)
    }
}

#[derive(Debug)]
pub enum FgdGenerationError {
    NonUtf8ConfigPath(PathBuf),
    ConfigManager(ConfigManagerError),
    Io(io::Error),
}

impl std::fmt::Display for FgdGenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonUtf8ConfigPath(path) => {
                write!(
                    f,
                    "OpenMW config path is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::ConfigManager(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for FgdGenerationError {}

impl From<ConfigManagerError> for FgdGenerationError {
    fn from(err: ConfigManagerError) -> Self {
        Self::ConfigManager(err)
    }
}

impl From<io::Error> for FgdGenerationError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

use std::collections::HashMap;

use crate::fgd::serialize::tag_to_tag_str;
type BoundsMap = HashMap<String, [i32; 6]>;

#[allow(
    clippy::cast_possible_truncation,
    reason = "FGD bounds are an integer format contract; fractional model coordinates are truncated at this boundary."
)]
fn fgd_bound(value: f32) -> i32 {
    value as i32
}

pub struct ConfigurationManager {
    merged_objects: BTreeMap<String, (TES3Object, String)>,
    object_bounds: BoundsMap,
    vfs: VFS,
    openmw_config: OpenMWConfiguration,
    object_types: HashSet<&'static str>,
    object_scale: f32,
}

/// Generate an FGD description from the configured `OpenMW` records.
///
/// # Errors
///
/// Returns an error when configuration, plugin, model, or output-file processing fails.
pub fn generate_fgd(
    config_path: Option<&Path>,
    object_types: &[&'static str],
    object_scale: f32,
    output_path: &Path,
) -> Result<(), FgdGenerationError> {
    // The public function deliberately preserves the command's fallible file/config boundary.
    let config_path =
        config_path.map_or_else(openmw_config::default_user_config_file, Path::to_path_buf);
    let config_path_str = config_path
        .to_str()
        .ok_or_else(|| FgdGenerationError::NonUtf8ConfigPath(config_path.clone()))?;
    let config_manager =
        ConfigurationManager::try_from_with_scale(config_path_str, object_types, object_scale)?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    serialize::serialize_objects_as_fgd(&config_manager, &mut writer)?;
    writer.flush()?;

    Ok(())
}

#[must_use]
pub fn get_object_model_path(object: &TES3Object) -> Option<String> {
    let mesh = match object {
        TES3Object::LeveledCreature(_)
        | TES3Object::LeveledItem(_)
        | TES3Object::Script(_)
        | TES3Object::StartScript(_) => {
            return None;
        }
        TES3Object::Activator(record) => &record.mesh,
        TES3Object::Alchemy(record) => &record.mesh,
        TES3Object::Apparatus(record) => &record.mesh,
        TES3Object::Armor(record) => &record.mesh,
        TES3Object::Book(record) => &record.mesh,
        TES3Object::Clothing(record) => &record.mesh,
        TES3Object::Door(record) => &record.mesh,
        TES3Object::Ingredient(record) => &record.mesh,
        TES3Object::Light(record) => &record.mesh,
        TES3Object::Lockpick(record) => &record.mesh,
        TES3Object::MiscItem(record) => &record.mesh,
        TES3Object::Probe(record) => &record.mesh,
        TES3Object::RepairItem(record) => &record.mesh,
        TES3Object::Static(record) => &record.mesh,
        TES3Object::Weapon(record) => &record.mesh,
        // TES3Object(record) => &record.mesh,
        _ => unimplemented!(
            "Unidentified object type in get_object_model_path: {}",
            object.tag_str()
        ),
    };

    if mesh == &String::default() {
        None
    } else {
        Some(mesh.to_ascii_lowercase())
    }
}

fn get_object_bounds_from_nif(
    vfs: &VFS,
    object: &TES3Object,
    object_scale: f32,
) -> Option<[i32; 6]> {
    let object_model = PathBuf::from("Meshes/").join(match object {
        TES3Object::Activator(record) => &record.mesh,
        TES3Object::Alchemy(record) => &record.mesh,
        TES3Object::Apparatus(record) => &record.mesh,
        TES3Object::Armor(record) => &record.mesh,
        TES3Object::Book(record) => &record.mesh,
        TES3Object::Clothing(record) => &record.mesh,
        TES3Object::Door(record) => &record.mesh,
        TES3Object::Ingredient(record) => &record.mesh,
        TES3Object::Light(record) => {
            if record.mesh == String::default() {
                return None;
            }
            &record.mesh
        }
        TES3Object::Lockpick(record) => &record.mesh,
        TES3Object::MiscItem(record) => &record.mesh,
        TES3Object::Probe(record) => &record.mesh,
        TES3Object::RepairItem(record) => &record.mesh,
        TES3Object::Script(_) | TES3Object::LeveledCreature(_) | TES3Object::LeveledItem(_) => {
            return None;
        }
        TES3Object::Static(record) => &record.mesh,
        TES3Object::Weapon(record) => &record.mesh,
        _ => unimplemented!(
            "Unimplemented object type in get_object_bounds_from_nif: {}",
            object.tag_str()
        ),
    });

    if let Some(vfs_file) = vfs.get_file(&object_model) {
        let mut bytes = Vec::new();
        if vfs_file
            .open()
            .and_then(|mut reader| reader.read_to_end(&mut bytes))
            .is_ok()
            && let Ok(stream) = tes3::nif::NiStream::from_bytes(&bytes)
        {
            if let Some((min, max)) = stream.bounding_box() {
                Some([
                    fgd_bound(min.x * object_scale),
                    fgd_bound(min.y * object_scale),
                    fgd_bound(min.z * object_scale),
                    fgd_bound(max.x * object_scale),
                    fgd_bound(max.y * object_scale),
                    fgd_bound(max.z * object_scale),
                ])
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    }
}

impl ConfigurationManager {
    /// Load and merge the configured plugins into the manager's object index.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured plugin cannot be found or parsed.
    pub fn collect_merged_objects(&mut self) -> Result<(), ConfigManagerError> {
        let content_files: Vec<&String> = self
            .openmw_config
            .content_files_iter()
            .map(openmw_config::FileSetting::value)
            .collect();

        content_files
            .par_iter()
            .rev()
            .map(|plugin_name| {
                if let Some(file) = self.vfs.get_file(plugin_name) {
                    if let Ok(plugin) = tes3::esp::Plugin::from_path_filtered(file.path(), |tag| {
                        serialize::is_serializable_tag(tag)
                            && self.object_types.contains(tag_to_tag_str(tag))
                    }) {
                        Ok((plugin, plugin_name))
                    } else {
                        Err(plugin_name)
                    }
                } else {
                    Err(plugin_name)
                }
            })
            .collect::<Vec<Result<(Plugin, &&String), &&String>>>()
            .into_iter()
            .try_for_each(|plugin| match plugin {
                Err(missing_plugin) => Err(ConfigManagerError::MissingPluginErr(
                    (*missing_plugin).clone(),
                )),
                Ok((plugin, plugin_name)) => {
                    plugin.objects.into_iter().for_each(|tes3_object| {
                        let editor_id = tes3_object.editor_id_ascii_lowercase().to_string();

                        if let Some(model) = get_object_model_path(&tes3_object)
                            && !self.object_bounds.contains_key(&model)
                            && let Some(bounds) = get_object_bounds_from_nif(
                                &self.vfs,
                                &tes3_object,
                                self.object_scale,
                            )
                        {
                            self.object_bounds.insert(model, bounds);
                        }

                        // Remember this won't work for cells :D
                        self.merged_objects
                            .entry(editor_id)
                            .or_insert_with(|| (tes3_object, plugin_name.to_ascii_lowercase()));
                    });
                    Ok(())
                }
            })
    }

    fn get_object_bounds(&self, mesh_path: &String) -> Option<&[i32; 6]> {
        self.object_bounds.get(mesh_path)
    }
}

impl TryFrom<(&str, &[&'static str])> for ConfigurationManager {
    type Error = ConfigManagerError;

    fn try_from((config_path, object_types): (&str, &[&'static str])) -> Result<Self, Self::Error> {
        Self::try_from_with_scale(config_path, object_types, 1.0)
    }
}

impl ConfigurationManager {
    /// Construct a manager from an `OpenMW` configuration path and object-type filter.
    ///
    /// # Errors
    ///
    /// Returns an error when the configuration cannot be loaded, the scale is invalid, or a
    /// configured plugin cannot be merged.
    pub fn try_from_with_scale(
        config_path: &str,
        object_types: &[&'static str],
        object_scale: f32,
    ) -> Result<Self, ConfigManagerError> {
        if !object_scale.is_finite() || object_scale <= 0.0 {
            return Err(ConfigManagerError::InvalidObjectScale(object_scale));
        }

        let config_path = PathBuf::from(config_path);

        let openmw_config = OpenMWConfiguration::new(Some(config_path))?;

        let vfs = VFS::from_directories(
            openmw_config
                .data_directories_iter()
                .map(openmw_config::DirectorySetting::parsed),
            Some(
                openmw_config
                    .fallback_archives_iter()
                    .map(|archive| archive.value().as_str())
                    .collect(),
            ),
        );

        let object_types: HashSet<&'static str> = object_types.iter().copied().collect();

        let mut manager = ConfigurationManager {
            vfs,
            openmw_config,
            object_types,
            object_scale,
            merged_objects: BTreeMap::new(),
            object_bounds: HashMap::new(),
        };

        manager.collect_merged_objects()?;

        Ok(manager)
    }
}

#[cfg(test)]
mod cfgmgr_test {
    use std::{
        fs::{self, File},
        io::BufWriter,
        path::PathBuf,
        sync::OnceLock,
    };

    use crate::fgd::{ConfigurationManager, generate_fgd, serialize};

    fn test_output_path(file_name: &str) -> PathBuf {
        let output_dir = PathBuf::from("fgd_out");
        fs::create_dir_all(&output_dir).expect("create FGD test output directory");
        output_dir.join(file_name)
    }

    fn test_config_path() -> PathBuf {
        static CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();
        CONFIG_PATH
            .get_or_init(|| {
                let path = test_output_path("openmw.cfg");
                fs::write(&path, "").expect("create empty OpenMW test configuration");
                path
            })
            .clone()
    }

    #[test]
    fn test_default_path() {
        let path = test_config_path();
        let object_types: &[&'static str] = &["NONE"];
        assert!(ConfigurationManager::try_from((path.to_str().unwrap(), object_types)).is_ok(),);
    }

    #[test]
    fn test_serialize_all() {
        let output_path = test_output_path("FGDOut_ALL.fgd");
        assert!(
            generate_fgd(
                Some(&test_config_path()),
                &serialize::SERIALIZABLE_TYPES,
                1.0,
                &output_path,
            )
            .is_ok()
        );
    }

    fn serialize_by_type(object_type: &'static str, config_path: Option<std::path::PathBuf>) {
        let path = config_path.unwrap_or_else(test_config_path);

        let types_slice: &[&'static str] = &[object_type];

        let config = ConfigurationManager::try_from((path.to_str().unwrap(), types_slice)).unwrap();

        let path = test_output_path(&format!("FGDOut_{object_type}.fgd"));
        let mut file = File::create(path).unwrap();
        let mut writer = BufWriter::new(&mut file);

        assert!(
            serialize::serialize_typed_objects_as_fgd(&config, &mut writer, object_type,).is_ok()
        );
    }

    #[test]
    fn test_serialize_static() {
        serialize_by_type(tes3::esp::Static::TAG_STR, None);
    }

    #[test]
    fn test_serialize_activator() {
        serialize_by_type(tes3::esp::Activator::TAG_STR, None);
    }

    #[test]
    fn test_serialize_script() {
        serialize_by_type(tes3::esp::Script::TAG_STR, None);
    }

    #[test]
    fn test_serialize_ingredient() {
        serialize_by_type(tes3::esp::Ingredient::TAG_STR, None);
    }

    #[test]
    fn test_serialize_light() {
        serialize_by_type(tes3::esp::Light::TAG_STR, None);
    }

    #[test]
    fn test_serialize_armor() {
        serialize_by_type(tes3::esp::Armor::TAG_STR, None);
    }

    #[test]
    fn test_serialize_weapon() {
        serialize_by_type(tes3::esp::Weapon::TAG_STR, None);
    }

    #[test]
    fn test_serialize_clothing() {
        serialize_by_type(tes3::esp::Armor::TAG_STR, None);
    }

    #[test]
    fn test_serialize_apparatus() {
        serialize_by_type(tes3::esp::Apparatus::TAG_STR, None);
    }

    #[test]
    fn test_serialize_potion() {
        serialize_by_type(tes3::esp::Alchemy::TAG_STR, None);
    }

    #[test]
    fn test_serialize_lockpick() {
        serialize_by_type(tes3::esp::Lockpick::TAG_STR, None);
    }

    #[test]
    fn test_serialize_probe() {
        serialize_by_type(tes3::esp::Probe::TAG_STR, None);
    }

    #[test]
    fn test_serialize_misc() {
        serialize_by_type(tes3::esp::MiscItem::TAG_STR, None);
    }

    #[test]
    fn test_serialize_repair() {
        serialize_by_type(tes3::esp::RepairItem::TAG_STR, None);
    }

    #[test]
    fn test_serialize_leveled_creature() {
        serialize_by_type(tes3::esp::LeveledCreature::TAG_STR, None);
    }

    #[test]
    fn test_serialize_leveled_item() {
        serialize_by_type(tes3::esp::LeveledItem::TAG_STR, None);
    }

    #[test]
    fn test_serialize_book() {
        serialize_by_type(tes3::esp::Book::TAG_STR, None);
    }

    #[test]
    fn test_serialize_door() {
        serialize_by_type(tes3::esp::Door::TAG_STR, None);
    }
}
