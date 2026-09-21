use rayon::iter::{IndexedParallelIterator, ParallelIterator};
use std::collections::BTreeMap;
use usage::Usage;

use super::TextureId;
use crate::slipgate::Textures;

pub enum TextureSizesTag {}

pub type TextureSizes = Usage<TextureSizesTag, BTreeMap<TextureId, (u32, u32)>>;

/// Construct using a name -> size map
pub fn texture_sizes(
    textures: &Textures,
    texture_sizes: BTreeMap<&str, (u32, u32)>,
) -> TextureSizes {
    textures
        .par_iter()
        .enumerate()
        .flat_map(|(texture_index, texture)| {
            texture_sizes
                .get(texture.as_str())
                .map(|texture_size| (TextureId(texture_index), *texture_size))
        })
        .collect()
}
