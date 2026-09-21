use std::fmt::Display;

use crate::slipgate::DenseId;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub usize);

impl DenseId for EntityId {
    fn index(self) -> usize {
        self.0
    }
}

impl Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.0, f)
    }
}
