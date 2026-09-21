mod texture_sizes;

pub use texture_sizes::*;

use crate::slipgate::DenseId;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureId(pub usize);

impl DenseId for TextureId {
    fn index(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for TextureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}
