mod entity;

pub use entity::*;

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

/// A Quake [`map`](https://www.gamers.org/dEngine/quake/QDP/qmapspec.html) containing one or more [`Entity`]s.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Map(pub Vec<Entity>);

impl Map {
    pub fn new(entities: Vec<Entity>) -> Self {
        entities.into()
    }
}

impl From<Vec<Entity>> for Map {
    fn from(entities: Vec<Entity>) -> Self {
        Map(entities)
    }
}

impl Deref for Map {
    type Target = Vec<Entity>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Map {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for Map {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some((last, rest)) = self.split_last() {
            for item in rest {
                writeln!(f, "{}", item)?;
            }
            write!(f, "{}", last)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_map_to_string() {
        assert_eq!(
            crate::slipgate::unit_test_data::test_map_out().to_string(),
            crate::slipgate::unit_test_data::test_map_in()
        )
    }
}
