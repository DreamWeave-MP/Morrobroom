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
    MissingTrenchBroomResources(PathBuf),
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
            Self::MissingTrenchBroomResources(path) => write!(
                f,
                "Cannot generate {GENERATED_FGD_NAME} because the Morrobroom TrenchBroom game configuration is not installed at:\n\n    {}\n\nExpected:\n    Morrowind.fgd\n    Nif.fgd\n    GameConfig.cfg\n\nInstall Morrobroom's TrenchBroom resources first, or use --output <path>.",
                path.display()
            ),
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
pub const GENERATED_FGD_NAME: &str = "MorrowindObjects.fgd";

/// Resolve the default location for the generated object catalog.
///
/// # Errors
///
/// Returns an error if the `TrenchBroom` user directory cannot be located or if
/// the Morrobroom game resources are not installed there.
pub fn default_catalog_output_path() -> Result<PathBuf, FgdGenerationError> {
    let game_dir = crate::platform::trenchbroom_morrowind_dir()?;
    catalog_output_path_in(game_dir)
}

fn catalog_output_path_in(game_dir: PathBuf) -> Result<PathBuf, FgdGenerationError> {
    let required_files = ["Morrowind.fgd", "Nif.fgd", "GameConfig.cfg"];
    if required_files
        .iter()
        .any(|file_name| !game_dir.join(file_name).is_file())
    {
        return Err(FgdGenerationError::MissingTrenchBroomResources(game_dir));
    }

    Ok(game_dir.join(GENERATED_FGD_NAME))
}

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
        Self::try_from_with_scale(config_path, object_types, 2.0)
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

        Self::from_openmw_config(openmw_config, object_types, object_scale)
    }

    fn from_openmw_config(
        openmw_config: OpenMWConfiguration,
        object_types: &[&'static str],
        object_scale: f32,
    ) -> Result<Self, ConfigManagerError> {
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
    use openmw_config::OpenMWConfiguration;

    use crate::fgd::{ConfigurationManager, serialize};

    fn empty_config_manager(object_types: &[&'static str]) -> ConfigurationManager {
        let openmw_config = OpenMWConfiguration::new_empty("empty-test-config")
            .expect("construct in-memory empty OpenMW configuration");
        ConfigurationManager::from_openmw_config(openmw_config, object_types, 1.0)
            .expect("construct manager from empty OpenMW configuration")
    }

    #[test]
    fn empty_openmw_config_does_not_require_a_config_file() {
        let manager = empty_config_manager(&["NONE"]);
        assert!(manager.merged_objects.is_empty());
    }

    #[test]
    fn test_serialize_all() {
        let manager = empty_config_manager(&serialize::SERIALIZABLE_TYPES);
        let mut generated = Vec::new();
        serialize::serialize_objects_as_fgd(&manager, &mut generated)
            .expect("serialize empty generated catalog");
        let generated = String::from_utf8(generated).expect("FGD output is valid UTF-8");
        assert!(generated.starts_with("@include \"Morrowind.fgd\"\n\n"));
    }
}

#[cfg(test)]
mod bundled_fgd_test {
    #[test]
    fn bundled_fgd_uses_valid_integer_properties() {
        let fgd = include_str!("../resources/Morrowind.fgd");
        let mut active_lines = fgd
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"));

        assert!(active_lines.all(|line| !line.contains("(int)")));
        assert!(fgd.contains("@BaseClass = WeaponData"));
        assert!(fgd.contains("@BaseClass base(Referenceable) = DoorData"));
        for property in [
            "ESM3_ChopMin(integer)",
            "ESM3_ChopMax(integer)",
            "ESM3_ThrustMin(integer)",
            "ESM3_ThrustMax(integer)",
            "ESM3_SlashMin(integer)",
            "ESM3_SlashMax(integer)",
            "ESM3_Health(integer)",
            "ESM3_Reach(float)",
            "ESM3_Speed(float)",
        ] {
            assert!(
                fgd.contains(property),
                "missing weapon schema property {property}"
            );
        }
        assert!(fgd.contains("ESM3_LightFlags(Flags)"));
        assert!(fgd.contains("ESM3_ContainerFlags(Flags)"));
        assert!(!fgd.contains("ESM3_LightFlags(string)"));
        assert!(!fgd.contains("ESM3_ContainerFlags(string)"));
        assert_eq!(
            fgd.matches("ESM3_Effect_7_Duration(integer)").count(),
            1,
            "Effect_7_Duration must be defined once"
        );
        assert_eq!(
            fgd.matches("ESM3_Effect_8_Duration(integer)").count(),
            1,
            "Effect_8_Duration must be defined once"
        );
    }
}

#[cfg(test)]
mod bundled_fgd_classes_test {
    use std::collections::HashSet;

    const BUNDLED: [&str; 2] = [
        include_str!("../resources/Morrowind.fgd"),
        include_str!("../resources/Nif.fgd"),
    ];

    /// Each active class declaration: its bases and its name.
    fn declarations() -> Vec<(Vec<String>, String)> {
        let active: String = BUNDLED
            .iter()
            .flat_map(|fgd| fgd.lines())
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        active
            .split('@')
            .skip(1)
            .filter_map(|declaration| {
                let header = declaration.split(['[', ':']).next()?;
                let name = header.split('=').nth(1)?.trim().to_owned();
                let bases = header
                    .split_once("base(")
                    .map(|(_, rest)| {
                        rest.split(')')
                            .next()
                            .unwrap_or_default()
                            .split(',')
                            .map(|base| base.trim().to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                Some((bases, name))
            })
            .collect()
    }

    #[test]
    fn every_base_class_a_bundled_class_names_is_declared() {
        let declarations = declarations();
        let names: HashSet<&str> = declarations.iter().map(|(_, name)| name.as_str()).collect();
        for (bases, name) in &declarations {
            for base in bases {
                assert!(
                    names.contains(base.as_str()),
                    "{name} names undeclared base {base}"
                );
            }
        }
    }

    #[test]
    fn each_body_part_slot_names_its_own_properties() {
        let fgd = BUNDLED[0];
        for slot in 1..=8 {
            let start = fgd
                .find(&format!("@BaseClass = BipedObject{slot} "))
                .unwrap_or_else(|| panic!("Morrowind.fgd should declare BipedObject{slot}"));
            let block = &fgd[start..start + fgd[start..].find("\n]").unwrap()];
            for property in ["SlotType", "male_part", "female_part"] {
                assert!(
                    block.contains(&format!("ESM3_{property}{slot}(")),
                    "BipedObject{slot} should declare ESM3_{property}{slot}"
                );
            }
        }
    }

    #[test]
    fn armor_and_clothing_are_brush_entities() {
        let declarations = declarations();
        for class in ["item_Armor", "item_Clothing"] {
            assert!(
                declarations.iter().any(|(_, name)| name == class),
                "Morrowind.fgd should declare {class}"
            );
        }
    }
}

#[cfg(test)]
mod bundled_nif_fgd_test {
    fn choices<'a>(fgd: &'a str, property: &str) -> Vec<&'a str> {
        let start = fgd
            .find(&format!("{property}(choices)"))
            .unwrap_or_else(|| panic!("Nif.fgd should declare {property}"));
        fgd[start..]
            .lines()
            .skip(2)
            .take_while(|line| !line.trim_start().starts_with(']'))
            .filter_map(|line| line.split(':').next())
            .map(str::trim)
            .collect()
    }

    #[test]
    fn billboard_modes_are_the_four_a_morrowind_nif_can_store() {
        let fgd = include_str!("../resources/Nif.fgd");
        assert_eq!(choices(fgd, "Nif_Billboard_Mode"), ["0", "1", "2", "3"]);
    }
}

#[cfg(test)]
mod default_catalog_path_test {
    use super::{FgdGenerationError, GENERATED_FGD_NAME, catalog_output_path_in};
    use std::process;

    #[test]
    fn default_catalog_path_requires_the_installed_game_resources() {
        let game_dir =
            std::env::temp_dir().join(format!("morrobroom-missing-tb-resources-{}", process::id()));
        let error = catalog_output_path_in(game_dir.clone()).unwrap_err();

        let FgdGenerationError::MissingTrenchBroomResources(path) = error else {
            panic!("expected missing-resource error");
        };
        assert_eq!(path, game_dir);
        let message = FgdGenerationError::MissingTrenchBroomResources(path).to_string();
        assert!(message.contains("Morrowind.fgd"));
        assert!(message.contains("Nif.fgd"));
        assert!(message.contains("GameConfig.cfg"));
        assert!(message.contains("use --output <path>"));
        assert_eq!(GENERATED_FGD_NAME, "MorrowindObjects.fgd");
    }
}
