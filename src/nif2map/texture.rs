#![allow(
    clippy::wildcard_imports,
    reason = "The texture resolver shares the parent importer types and VFS contract."
)]

use super::*;

pub(super) struct TextureResolver {
    vfs: VFS,
}

impl TextureResolver {
    pub(super) fn new(roots: &[PathBuf]) -> io::Result<Self> {
        let mut archives = Vec::new();
        let mut directories = Vec::new();
        for root in roots {
            if root.is_dir() {
                directories.push(root);
            } else {
                archives.push(root);
            }
        }

        // OpenMW registers configured archives first and loose data directories second. The
        // latter therefore always override the former, while order is preserved within each
        // class. `texture-roots` is the source of truth for both classes here.
        let mut vfs = VFS::new();
        for archive in archives {
            if !vfs.push_archive(archive) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("could not open texture archive {}", archive.display()),
                ));
            }
        }
        for directory in directories {
            vfs.push_directory(directory)?;
        }
        Ok(Self { vfs })
    }

    pub(super) fn texture_candidates(source_texture: &str) -> Vec<String> {
        let normalized = source_texture.replace('\\', "/").to_ascii_lowercase();
        let corrected = ["textures", "bookart"]
            .iter()
            .find_map(|directory| {
                normalized
                    .split('/')
                    .position(|component| component == *directory)
                    .map(|index| {
                        normalized
                            .split('/')
                            .skip(index)
                            .collect::<Vec<_>>()
                            .join("/")
                    })
            })
            .unwrap_or_else(|| format!("textures/{normalized}"));
        let original = corrected.clone();
        let changed = corrected
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension != "dds");
        let dds = if changed {
            let mut path = corrected.clone();
            if let Some(dot) = path.rfind('.') {
                path.truncate(dot);
            }
            path.push_str(".dds");
            Some(path)
        } else {
            None
        };
        let filename = Path::new(&original)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&original);
        let mut candidates = Vec::with_capacity(4);
        if let Some(dds) = dds {
            candidates.push(dds);
        }
        candidates.push(original.clone());
        if changed {
            let mut flat_dds = filename.to_owned();
            if let Some(dot) = flat_dds.rfind('.') {
                flat_dds.truncate(dot);
            }
            flat_dds.push_str(".dds");
            candidates.push(format!("textures/{flat_dds}"));
        }
        candidates.push(format!("textures/{filename}"));
        candidates.dedup();
        candidates
    }

    pub(super) fn resolve(&self, source_texture: &str) -> Result<TextureDimensions, String> {
        let candidates = Self::texture_candidates(source_texture);
        let Some((resolved, file)) = candidates.iter().find_map(|candidate| {
            self.vfs
                .get_file(candidate)
                .map(|file| (candidate.as_str(), file))
        }) else {
            return Err(format!(
                "texture {source_texture:?} was not found in the texture VFS (tried {})",
                candidates.join(", ")
            ));
        };
        let mut reader = file.open().map_err(|error| {
            format!(
                "texture {source_texture:?} resolved to {resolved}, but could not be read: {error}"
            )
        })?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut reader, &mut bytes).map_err(|error| {
            format!(
                "texture {source_texture:?} resolved to {resolved}, but could not be read: {error}"
            )
        })?;
        let dimensions = blob_size(&bytes).map_err(|error| {
            format!(
                "texture {source_texture:?} resolved to {resolved} has unreadable dimensions: {error}"
            )
        })?;
        Ok(TextureDimensions {
            size: (
                u32::try_from(dimensions.width)
                    .map_err(|_| format!("texture {source_texture:?} is too wide"))?,
                u32::try_from(dimensions.height)
                    .map_err(|_| format!("texture {source_texture:?} is too tall"))?,
            ),
            source: TextureSizeSource::Resolved,
        })
    }

    #[cfg(test)]
    pub(super) fn dimensions(&self, source_texture: &str) -> (u32, u32) {
        self.resolve(source_texture)
            .expect("texture should resolve")
            .size
    }

    #[cfg(test)]
    pub(super) fn resolved_path(&self, source_texture: &str) -> String {
        Self::texture_candidates(source_texture)
            .into_iter()
            .find(|candidate| self.vfs.get_file(candidate).is_some())
            .expect("texture should resolve")
    }
}
