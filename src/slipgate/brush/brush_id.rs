use std::fmt::Display;

use crate::slipgate::DenseId;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BrushId(pub usize);

impl Display for BrushId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl DenseId for BrushId {
    fn index(self) -> usize {
        self.0
    }
}
