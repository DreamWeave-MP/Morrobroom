use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};

/// Value Parser for input scale
fn validate_scale(arg: &str) -> Result<f32, String> {
    arg.parse::<f32>()
        .map_err(|e| format!("Invalid scale value '{arg}': {e}"))
        .and_then(|num| {
            if num <= 0.0 {
                Err("Scale value must be greater than 0".to_string())
            } else {
                Ok(num)
            }
        })
}

fn validate_input_map(s: &str) -> Result<PathBuf, String> {
    let path = Path::new(s);

    let meta = fs::metadata(path).map_err(|e| format!("Map file does not exist: {s} ({e})"))?;
    if !meta.is_file() {
        return Err(format!("Provided path is not a regular file: {s}"));
    }

    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("map") => {}
        Some(ext) => return Err(format!("Invalid extension: .{ext} (expected .map)")),
        None => return Err("Map file is missing an extension".into()),
    }

    let abs_path = fs::canonicalize(path)
        .map_err(|e| format!("Failed to canonicalize map path: {s} ({e})"))?;

    Ok(abs_path)
}

fn validate_compile_output_path(s: &str) -> Result<PathBuf, String> {
    let path = Path::new(s);

    let allowed_exts = ["esp", "esm", "omwaddon", "omwgame"];
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or("Output file must have a valid extension (esp, esm, omwaddon, omwgame)")?;

    if !allowed_exts
        .iter()
        .any(|&allowed| ext.eq_ignore_ascii_case(allowed))
    {
        return Err(format!(
            "Invalid output extension: .{} (must be one of: {})",
            ext,
            allowed_exts.join(", ")
        ));
    }

    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {e}"))?
            .join(path)
    };

    let parent = abs_path.parent().ok_or_else(|| {
        format!(
            "Output path has no parent directory: {}",
            abs_path.display()
        )
    })?;
    fs::create_dir_all(parent).map_err(|e| {
        format!(
            "Failed to create output directory {}: {e}",
            parent.display()
        )
    })?;
    let canonical_parent = fs::canonicalize(parent).map_err(|e| {
        format!(
            "Failed to canonicalize output directory {}: {e}",
            parent.display()
        )
    })?;
    let file_name = abs_path
        .file_name()
        .ok_or_else(|| format!("Output path has no file name: {}", abs_path.display()))?;

    Ok(canonical_parent.join(file_name))
}

fn validate_openmw_config_path(s: &str) -> Result<PathBuf, String> {
    let path = Path::new(s);

    fs::metadata(path)
        .map_err(|e| format!("OpenMW config file does not exist or is inaccessible: {s} ({e})"))?;

    let abs_path = fs::canonicalize(path)
        .map_err(|e| format!("Failed to canonicalize OpenMW config path: {s} ({e})"))?;

    Ok(abs_path)
}

pub const fn default_scale() -> &'static str {
    "2.0"
}

pub const fn default_object_types() -> [TES3ObjectType; 18] {
    [
        TES3ObjectType::ACTI,
        TES3ObjectType::ALCH,
        TES3ObjectType::APPA,
        TES3ObjectType::ARMO,
        TES3ObjectType::BOOK,
        TES3ObjectType::CLOT,
        TES3ObjectType::DOOR,
        TES3ObjectType::INGR,
        TES3ObjectType::LEVC,
        TES3ObjectType::LEVI,
        TES3ObjectType::LIGH,
        TES3ObjectType::LOCK,
        TES3ObjectType::MISC,
        TES3ObjectType::PROB,
        TES3ObjectType::REPA,
        TES3ObjectType::SCPT,
        TES3ObjectType::STAT,
        TES3ObjectType::WEAP,
    ]
}

#[derive(Debug, Parser)]
#[command(
    name = "morrobroom",
    about = "Compile trenchbroom .map files into usable Morrowind plugins and NIFs.",
    arg_required_else_help = true
)]
pub struct MorrobroomArgs {
    #[command(subcommand)]
    pub command: BroomCommand,
}

#[derive(Debug, Subcommand)]
pub enum BroomCommand {
    #[command(
        about = "Compile a TrenchBroom map into NIF meshes, a baked lightmap and a TES3 plugin"
    )]
    Compile {
        /// Input map file name.
        #[arg(long = "map", required = true, value_parser = validate_input_map )]
        map_path: PathBuf,

        /// Scales generated meshes by this value. Defaults to 2.0, but can be useful when working with Quake maps
        /// which are approximately 50% the scale of Morrowind assets.
        #[arg(long = "scale", short = 's', value_parser = validate_scale, default_value = default_scale())]
        object_scale: f32,

        /// `OpenMW` root or user configuration file/directory. When omitted, use
        /// `openmw-config` root discovery with its user-config fallback.
        #[arg(long = "config", short = 'c', value_parser = validate_openmw_config_path)]
        openmw_config: Option<PathBuf>,

        /// Name of the plugin used when serializing.
        /// If not present, defaults to the name of the map file used in generation.
        #[arg(long = "output", short = 'o', value_parser = validate_compile_output_path)]
        output_path: Option<PathBuf>,

        /// Root directory for generated meshes and lightmaps.
        /// If omitted, generated assets are written beside the input map.
        #[arg(long = "output-dir")]
        output_dir: Option<PathBuf>,

        /// Disable generated lightmap UVs and lightmap baking.
        #[arg(long = "no-lightmaps")]
        no_lightmaps: bool,
    },
    #[command(
        name = "nif2map",
        about = "Reverse-compile Morrowind NetImmerse visual geometry into Valve 220 maps"
    )]
    Nif2Map {
        /// NIF files or directories to convert.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Recurse into input directories.
        #[arg(long, short = 'r')]
        recursive: bool,

        /// Output directory for .map and .nif2map.json files.
        #[arg(long = "output-dir", short = 'o', default_value = "nif2map-out")]
        output_dir: PathBuf,

        /// Backing thickness for open swept architectural shells, in map units.
        #[arg(long, default_value_t = 8.0)]
        shell_thickness: f64,

        /// Backing thickness for planar-region fallback, in map units.
        #[arg(long, default_value_t = 1.0)]
        fallback_thickness: f64,

        /// Unsupported-shape behavior: planar-prisms or skip.
        #[arg(long, default_value = "planar-prisms")]
        fallback: String,

        /// Material for artificial closure and partition faces.
        #[arg(long, default_value = "skip")]
        skip_material: String,

        /// Ordered `OpenMW` VFS sources: data directories or existing .bsa, .ba2, or .zip archives.
        /// Repeatable; later directories/archives override earlier sources within their class.
        #[arg(long = "texture-root", required = true)]
        texture_roots: Vec<PathBuf>,

        /// Import `RootCollisionNode` descendants into a separate Collision authoring group.
        #[arg(long)]
        include_collision: bool,

        /// Do not reject already existing outputs.
        #[arg(long)]
        overwrite: bool,

        /// Analyze and report without writing map files.
        #[arg(long)]
        dry_run: bool,

        /// Print recognizer details and warnings.
        #[arg(long, short = 'v')]
        verbose: bool,

        /// Do not validate generated brushes.
        #[arg(long)]
        no_validate: bool,

        /// Permit at most this many generated brushes per input.
        #[arg(long, default_value_t = 20_000)]
        max_brushes: usize,

        /// Write map units: the NIF's Morrowind units divided by this. Defaults to 2.0, the
        /// scale compile multiplies by, so compiling the map at the same scale gives the NIF's size.
        #[arg(long = "scale", short = 's', value_parser = validate_scale, default_value = default_scale())]
        object_scale: f32,
    },
    #[command(
        about = "Generate a TrenchBroom entity catalog, MorrowindObjects.fgd, from an OpenMW load order"
    )]
    Fgd {
        /// Scale to use when generating object bounding boxes.
        /// Useful if authoring at a different scale, for one or another reason.
        #[arg(long = "scale", short = 's', value_parser = validate_scale, default_value = default_scale())]
        object_scale: f32,

        /// List of all object types to use in FGD serialization.
        /// All of these types will be used in writing a single, merged FGD file.
        /// Uses the four-letter shortnames defined by the TES3 ESP format. Refer to
        /// <https://en.uesp.net/wiki/Morrowind_Mod:Mod_File_Format>
        ///  for more specific examples of what types to use. Does not support cells or dialogues,
        /// only objects which are otherwise placeable in the game world.
        #[arg(long = "types", short = 't', value_delimiter = ';', ignore_case = true)]
        object_types: Option<Vec<TES3ObjectType>>,

        /// Output path for the generated object catalog. Defaults to the installed
        /// `TrenchBroom` Morrowind game directory.
        #[arg(long = "output", short = 'o')]
        output_path: Option<PathBuf>,

        /// Relative or absolute path to the openmw.cfg file from which to derive the FGD file.
        #[arg(long = "config", short = 'c', value_parser = validate_openmw_config_path)]
        openmw_config: Option<PathBuf>,
    },
}

#[allow(
    clippy::upper_case_acronyms,
    reason = "The four-letter variants are the public TES3 record tags accepted by the CLI."
)]
#[derive(Clone, Debug, ValueEnum)]
pub enum TES3ObjectType {
    ACTI,
    ALCH,
    APPA,
    ARMO,
    BOOK,
    CLOT,
    DOOR,
    INGR,
    LEVC,
    LEVI,
    LIGH,
    LOCK,
    MISC,
    PROB,
    REPA,
    SCPT,
    STAT,
    WEAP,
}

impl TES3ObjectType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            TES3ObjectType::ACTI => tes3::esp::Activator::TAG_STR,
            TES3ObjectType::ALCH => tes3::esp::Alchemy::TAG_STR,
            TES3ObjectType::APPA => tes3::esp::Apparatus::TAG_STR,
            TES3ObjectType::ARMO => tes3::esp::Armor::TAG_STR,
            TES3ObjectType::BOOK => tes3::esp::Book::TAG_STR,
            TES3ObjectType::CLOT => tes3::esp::Clothing::TAG_STR,
            TES3ObjectType::DOOR => tes3::esp::Door::TAG_STR,
            TES3ObjectType::INGR => tes3::esp::Ingredient::TAG_STR,
            TES3ObjectType::LEVC => tes3::esp::LeveledCreature::TAG_STR,
            TES3ObjectType::LEVI => tes3::esp::LeveledItem::TAG_STR,
            TES3ObjectType::LIGH => tes3::esp::Light::TAG_STR,
            TES3ObjectType::LOCK => tes3::esp::Lockpick::TAG_STR,
            TES3ObjectType::MISC => tes3::esp::MiscItem::TAG_STR,
            TES3ObjectType::PROB => tes3::esp::Probe::TAG_STR,
            TES3ObjectType::REPA => tes3::esp::RepairItem::TAG_STR,
            TES3ObjectType::SCPT => tes3::esp::Script::TAG_STR,
            TES3ObjectType::STAT => tes3::esp::Static::TAG_STR,
            TES3ObjectType::WEAP => tes3::esp::Weapon::TAG_STR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use std::{
        env,
        fs::{self, File},
        io::Write,
        path::{Path, PathBuf},
        sync::{Mutex, MutexGuard},
        time::{SystemTime, UNIX_EPOCH},
    };

    static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

    struct CurrentDirGuard {
        _guard: MutexGuard<'static, ()>,
        original_dir: PathBuf,
    }

    impl CurrentDirGuard {
        fn set_to(path: impl AsRef<Path>) -> Self {
            let guard = CURRENT_DIR_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let original_dir = env::current_dir().unwrap();
            env::set_current_dir(path).unwrap();

            Self {
                _guard: guard,
                original_dir,
            }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original_dir);
        }
    }

    /// Create a temporary file (optionally with contents) and return its `PathBuf`.
    /// Create a temporary file with a given extension and contents.
    fn temp_file(ext: &str, contents: &[u8]) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_millis();
        let file_path = env::temp_dir().join(format!("mrb_test_{now}.{ext}"));
        let mut f = File::create(&file_path).unwrap();
        f.write_all(contents).unwrap();
        file_path
    }

    /// Create a unique temporary directory.
    fn temp_dir() -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_millis();
        let dir = env::temp_dir().join(format!("mrb_dir_{now}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scale_parser_accepts_positive() {
        assert!((validate_scale("1.25").unwrap() - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn scale_parser_rejects_zero_or_negative() {
        assert!(validate_scale("0").is_err());
        assert!(validate_scale("-3.0").is_err());
    }

    #[test]
    fn input_map_parser_accepts_real_map() {
        let map_file = temp_file("map", b"dummy");
        let parsed = validate_input_map(map_file.to_str().unwrap()).unwrap();
        assert!(parsed.is_absolute());
        assert_eq!(parsed.extension().unwrap(), "map");
    }

    #[test]
    fn input_map_parser_rejects_wrong_extension() {
        let bad_file = temp_file("txt", b"dummy");
        assert!(validate_input_map(bad_file.to_str().unwrap()).is_err());
    }

    #[test]
    fn output_path_parser_creates_parent_dirs() {
        let tmp_dir = temp_dir();
        let _guard = CurrentDirGuard::set_to(&tmp_dir);

        let rel_out = Path::new("new_dir/sub/plugin.esp");
        let out_path = validate_compile_output_path(rel_out.to_str().unwrap()).unwrap();

        assert!(out_path.is_absolute());
        assert!(out_path.ends_with("plugin.esp"));
        assert!(out_path.parent().unwrap().exists());
    }

    #[test]
    fn output_path_parser_rejects_bad_extension() {
        let err = validate_compile_output_path("out.bad").unwrap_err();
        assert!(err.contains("Invalid output extension"));
    }

    #[test]
    fn tes3objecttype_as_str_matches_expected_tags() {
        use TES3ObjectType::*;
        let tag_pairs = [
            (ACTI, tes3::esp::Activator::TAG_STR),
            (ALCH, tes3::esp::Alchemy::TAG_STR),
            (APPA, tes3::esp::Apparatus::TAG_STR),
            (ARMO, tes3::esp::Armor::TAG_STR),
            (BOOK, tes3::esp::Book::TAG_STR),
            (CLOT, tes3::esp::Clothing::TAG_STR),
            (DOOR, tes3::esp::Door::TAG_STR),
            (INGR, tes3::esp::Ingredient::TAG_STR),
            (LEVC, tes3::esp::LeveledCreature::TAG_STR),
            (LEVI, tes3::esp::LeveledItem::TAG_STR),
            (LIGH, tes3::esp::Light::TAG_STR),
            (LOCK, tes3::esp::Lockpick::TAG_STR),
            (MISC, tes3::esp::MiscItem::TAG_STR),
            (PROB, tes3::esp::Probe::TAG_STR),
            (REPA, tes3::esp::RepairItem::TAG_STR),
            (SCPT, tes3::esp::Script::TAG_STR),
            (STAT, tes3::esp::Static::TAG_STR),
            (WEAP, tes3::esp::Weapon::TAG_STR),
        ];

        for (variant, expected_tag) in tag_pairs {
            assert_eq!(
                variant.as_str(),
                expected_tag,
                "Mismatched tag for {variant:?}"
            );
        }
    }

    #[test]
    fn clap_compile_subcommand_parses() {
        let map_file = temp_file("map", b"dummy");
        let tmp_out = temp_file("esp", b"").with_extension("esp");
        let config_file = temp_file("cfg", b"");

        let args = MorrobroomArgs::parse_from([
            "morrobroom",
            "compile",
            "--map",
            map_file.to_str().unwrap(),
            "--scale",
            "2.0",
            "--config",
            config_file.to_str().unwrap(),
            "--output",
            tmp_out.to_str().unwrap(),
        ]);

        match args.command {
            BroomCommand::Compile {
                map_path,
                object_scale,
                openmw_config,
                output_path,
                output_dir,
                no_lightmaps,
            } => {
                assert!((object_scale - 2.0).abs() < f32::EPSILON);

                assert_eq!(
                    map_path.canonicalize().unwrap(),
                    map_file.canonicalize().unwrap()
                );

                assert_eq!(output_path, Some(tmp_out.canonicalize().unwrap()));
                assert_eq!(openmw_config, Some(config_file.canonicalize().unwrap()));
                assert!(output_dir.is_none());
                assert!(!no_lightmaps);
            }
            BroomCommand::Fgd { .. } | BroomCommand::Nif2Map { .. } => {
                panic!("expected compile subcommand")
            }
        }
    }

    #[test]
    fn compile_defaults_to_morrowind_scale() {
        let map_file = temp_file("map", b"dummy");
        let args = MorrobroomArgs::parse_from([
            "morrobroom",
            "compile",
            "--map",
            map_file.to_str().unwrap(),
        ]);

        let BroomCommand::Compile { object_scale, .. } = args.command else {
            panic!("expected compile subcommand");
        };
        assert!((object_scale - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compile_command_can_disable_lightmaps() {
        let map_file = temp_file("map", b"dummy");
        let args = MorrobroomArgs::parse_from([
            "morrobroom",
            "compile",
            "--map",
            map_file.to_str().unwrap(),
            "--no-lightmaps",
        ]);

        match args.command {
            BroomCommand::Compile {
                no_lightmaps,
                openmw_config,
                ..
            } => {
                assert!(no_lightmaps);
                assert!(openmw_config.is_none());
            }
            BroomCommand::Fgd { .. } | BroomCommand::Nif2Map { .. } => {
                panic!("expected compile subcommand")
            }
        }
    }

    #[test]
    fn compile_command_accepts_bare_relative_map_from_current_dir() {
        let tmp_dir = temp_dir();
        let map_file = tmp_dir.join("test.map");
        File::create(&map_file)
            .unwrap()
            .write_all(b"dummy")
            .unwrap();
        let expected_map_path = fs::canonicalize(&map_file).unwrap();

        let _guard = CurrentDirGuard::set_to(&tmp_dir);

        let result = MorrobroomArgs::try_parse_from(["morrobroom", "compile", "--map", "test.map"]);

        let args = result.expect("bare relative map should parse from current directory");
        match args.command {
            BroomCommand::Compile { map_path, .. } => {
                assert_eq!(map_path, expected_map_path);
            }
            BroomCommand::Fgd { .. } | BroomCommand::Nif2Map { .. } => {
                panic!("expected compile subcommand")
            }
        }
    }

    #[test]
    fn top_level_help_describes_every_command() {
        let help = MorrobroomArgs::command().render_help().to_string();

        for (command, description) in [
            ("compile", "Compile a TrenchBroom map"),
            ("nif2map", "Reverse-compile Morrowind"),
            ("fgd", "Generate a TrenchBroom entity catalog"),
        ] {
            let line = help
                .lines()
                .find(|line| line.trim_start().starts_with(command))
                .unwrap_or_else(|| panic!("help does not list {command}:\n{help}"));
            assert!(
                line.contains(description),
                "help has no description for {command}: {line:?}"
            );
        }
    }

    #[test]
    fn top_level_map_flag_is_rejected() {
        let result = MorrobroomArgs::try_parse_from(["morrobroom", "--map", "cubetest.map"]);

        assert!(
            result.is_err(),
            "top-level --map is not part of the CLI contract"
        );
    }

    #[test]
    fn top_level_help_mentions_subcommands_not_old_positional_map() {
        let help = MorrobroomArgs::command().render_help().to_string();

        assert!(!help.contains("Path/to/Map_Name.map"));
        assert!(help.contains("Usage: morrobroom <COMMAND>"));
        assert!(help.contains("compile"));
    }

    #[test]
    fn fgd_types_accept_record_tags_in_either_case() {
        let args = MorrobroomArgs::parse_from(["morrobroom", "fgd", "--types", "STAT;door;LiGh"]);

        let BroomCommand::Fgd { object_types, .. } = args.command else {
            panic!("expected FGD subcommand");
        };
        assert_eq!(object_types.expect("types were given").len(), 3);
    }

    #[test]
    fn nif2map_writes_map_units_at_the_compile_scale() {
        let args = MorrobroomArgs::parse_from([
            "morrobroom",
            "nif2map",
            "crate.nif",
            "--texture-root",
            ".",
        ]);

        let BroomCommand::Nif2Map { object_scale, .. } = args.command else {
            panic!("expected nif2map subcommand");
        };
        assert!((object_scale - 2.0).abs() < f32::EPSILON);
        assert!(
            MorrobroomArgs::try_parse_from([
                "morrobroom",
                "nif2map",
                "crate.nif",
                "--texture-root",
                ".",
                "--scale",
                "0",
            ])
            .is_err()
        );
    }

    #[test]
    fn catalog_command_is_spelled_fgd() {
        assert!(MorrobroomArgs::try_parse_from(["morrobroom", "fgd"]).is_ok());
        assert!(MorrobroomArgs::try_parse_from(["morrobroom", "FGD"]).is_err());
    }

    #[test]
    fn clap_fgd_defaults() {
        let args = MorrobroomArgs::parse_from(["morrobroom", "fgd", "--scale", "3.5"]);

        if let BroomCommand::Fgd {
            object_scale,
            object_types,
            output_path,
            openmw_config,
        } = args.command
        {
            let types = object_types.unwrap_or(default_object_types().into());

            assert!((object_scale - 3.5).abs() < f32::EPSILON);
            assert_eq!(types.len(), 18);
            assert!(openmw_config.is_none());
            assert!(output_path.is_none());
        } else {
            panic!("expected FGD subcommand");
        }
    }

    #[test]
    fn fgd_output_path_can_be_overridden() {
        let args = MorrobroomArgs::parse_from([
            "morrobroom",
            "fgd",
            "--output",
            "portable/MorrowindObjects.fgd",
        ]);

        let BroomCommand::Fgd { output_path, .. } = args.command else {
            panic!("expected FGD subcommand");
        };
        assert_eq!(
            output_path,
            Some(PathBuf::from("portable/MorrowindObjects.fgd"))
        );
    }

    fn temp_map_file() -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let path = std::env::temp_dir().join(format!("test_map_{}.map", now.as_nanos()));

        let mut file = File::create(&path).expect("Could not create temp map file");
        writeln!(file, "// dummy map").unwrap();

        path
    }

    #[test]
    fn compile_command_fails_for_missing_map() {
        let result = MorrobroomArgs::try_parse_from([
            "morrobroom",
            "compile",
            "--map",
            "path/that/does/not/exist.map",
            "--output",
            "Test.omwaddon",
        ]);

        assert!(result.is_err(), "Expected failure for missing map file");
    }

    #[test]
    fn compile_command_succeeds_for_existing_map() {
        let path = temp_map_file();

        let result = MorrobroomArgs::try_parse_from([
            "morrobroom",
            "compile",
            "--map",
            path.to_str().unwrap(),
            "--output",
            "Test.omwaddon",
        ]);

        assert!(
            result.is_ok(),
            "Expected successful parse with valid map file"
        );

        // Optionally inspect command output
        if let Ok(MorrobroomArgs {
            command: BroomCommand::Compile { map_path, .. },
        }) = result
        {
            assert_eq!(
                map_path,
                std::fs::canonicalize(path).unwrap(),
                "Parsed path should be canonical"
            );
        }
    }
}
