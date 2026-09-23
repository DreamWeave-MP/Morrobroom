use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
use std::env;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn append_game_directory(root: &Path) -> PathBuf {
    root.join("games").join("Morrowind")
}

#[cfg(target_os = "windows")]
fn game_directory_from_user_root(root: &Path) -> PathBuf {
    append_game_directory(&root.join("TrenchBroom"))
}

#[cfg(target_os = "macos")]
fn game_directory_from_user_root(root: &Path) -> PathBuf {
    append_game_directory(
        &root
            .join("Library")
            .join("Application Support")
            .join("TrenchBroom"),
    )
}

#[cfg(target_os = "linux")]
fn game_directory_from_user_root(root: &Path) -> PathBuf {
    append_game_directory(&root.join(".TrenchBroom"))
}

/// Resolve `TrenchBroom`'s per-user Morrowind game directory for this platform.
///
/// # Errors
///
/// Returns an error when the platform-specific user data root cannot be found.
pub fn trenchbroom_morrowind_dir() -> io::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata =
            env::var_os("APPDATA").ok_or_else(|| missing_environment_variable("APPDATA"))?;
        let appdata = PathBuf::from(appdata);
        Ok(game_directory_from_user_root(&appdata))
    }

    #[cfg(target_os = "macos")]
    {
        let home = env::var_os("HOME").ok_or_else(|| missing_environment_variable("HOME"))?;
        let home = PathBuf::from(home);
        Ok(game_directory_from_user_root(&home))
    }

    #[cfg(target_os = "linux")]
    {
        let home = env::var_os("HOME").ok_or_else(|| missing_environment_variable("HOME"))?;
        let home = PathBuf::from(home);
        Ok(game_directory_from_user_root(&home))
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TrenchBroom's custom game directory is not defined for this operating system",
        ))
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn missing_environment_variable(name: &'static str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "cannot locate TrenchBroom user data: environment variable {name} is not set; set it or use --output <path>"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn appends_the_morrowind_game_directory() {
        let root = PathBuf::from("/test/user-data/TrenchBroom");
        assert_eq!(
            append_game_directory(&root),
            PathBuf::from("/test/user-data/TrenchBroom/games/Morrowind")
        );
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    fn platform_path_uses_the_platform_specific_user_root() {
        let root = PathBuf::from("/test/user-data");
        let path = game_directory_from_user_root(&root);

        #[cfg(target_os = "windows")]
        assert_eq!(
            path,
            PathBuf::from("/test/user-data/TrenchBroom/games/Morrowind")
        );
        #[cfg(target_os = "macos")]
        assert_eq!(
            path,
            PathBuf::from(
                "/test/user-data/Library/Application Support/TrenchBroom/games/Morrowind"
            )
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            path,
            PathBuf::from("/test/user-data/.TrenchBroom/games/Morrowind")
        );
    }
}
