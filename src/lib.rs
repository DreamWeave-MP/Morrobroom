use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

pub mod fgd;
pub mod slipgate;

pub mod util;
pub use util::FindLowest;

pub fn get_prop<'a>(
    prop_name: &str,
    prop_map: &'a HashMap<&String, &String>,
) -> Option<&'a String> {
    prop_map
        .iter()
        .find(|(k, _)| k.as_str() == prop_name)
        .map(|(_, v)| *v)
}

/// Create the working directory structure for the map export.
///
/// Returns a tuple of (workdir root path, map subfolder name).
pub fn create_workdir(map_path: &Path) -> Result<(PathBuf, String), String> {
    let file_stem = map_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Failed to extract file name from map path")?;

    let parent = map_path
        .parent()
        .ok_or("Provided map path has no parent directory")?;

    let workdir = parent.to_path_buf();
    let meshes_dir = workdir.join("Meshes");
    let map_dir = meshes_dir.join(file_stem);

    // Create the full directory tree in one go
    fs::create_dir_all(&map_dir).map_err(|e| format!("Failed to create workdir: {}", e))?;

    Ok((workdir, file_stem.to_string()))
}
