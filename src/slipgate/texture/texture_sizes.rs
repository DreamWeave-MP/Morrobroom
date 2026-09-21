use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::collections::BTreeMap;
use usage::Usage;

use super::TextureId;

pub enum TextureSizesTag {}

pub type TextureSizes = Usage<TextureSizesTag, BTreeMap<TextureId, (u32, u32)>>;

/// Construct using a name -> size map
pub fn texture_sizes(
    textures: &BTreeMap<TextureId, String>,
    texture_sizes: BTreeMap<&str, (u32, u32)>,
) -> TextureSizes {
    textures
        .par_iter()
        .flat_map(|(texture_id, texture)| {
            texture_sizes
                .get(texture.as_str())
                .map(|texture_size| (*texture_id, *texture_size))
        })
        .collect()
}
