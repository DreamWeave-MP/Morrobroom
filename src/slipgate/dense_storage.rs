use std::{
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use rayon::{iter::IntoParallelRefIterator, slice::Iter as RayonIter};

/// A dense, typed index into [`DenseStorage`].
pub trait DenseId: Copy {
    fn index(self) -> usize;
}

/// Assert that an ID sequence is exactly `0..len`.
///
/// # Panics
///
/// Panics when an ID is missing, duplicated, or out of order.
pub fn assert_contiguous_ids<K: DenseId>(ids: impl IntoIterator<Item = K>) {
    for (expected, id) in ids.into_iter().enumerate() {
        assert_eq!(
            id.index(),
            expected,
            "dense ID invariant violated: expected index {expected}, got {}",
            id.index()
        );
    }
}

/// Contiguous storage indexed by a typed dense ID.
///
/// The type parameter prevents accidentally indexing one arena with another
/// arena's ID while keeping the representation a plain `Vec<T>`.
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
pub struct DenseStorage<K, V> {
    values: Vec<V>,
    marker: PhantomData<K>,
}

impl<K, V> DenseStorage<K, V> {
    #[must_use]
    pub fn from_vec(values: Vec<V>) -> Self {
        Self {
            values,
            marker: PhantomData,
        }
    }

    /// Build storage from explicitly keyed values, rejecting holes or
    /// out-of-order IDs instead of silently assigning the wrong value.
    ///
    /// # Panics
    ///
    /// Panics when a supplied ID is not the next contiguous index.
    pub fn from_pairs<I>(pairs: I) -> Self
    where
        K: DenseId,
        I: IntoIterator<Item = (K, V)>,
    {
        let mut values = Vec::new();
        for (id, value) in pairs {
            assert_eq!(
                id.index(),
                values.len(),
                "dense ID invariant violated: expected index {}, got {}",
                values.len(),
                id.index()
            );
            values.push(value);
        }
        Self::from_vec(values)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[V] {
        &self.values
    }

    pub fn as_mut_slice(&mut self) -> &mut [V] {
        &mut self.values
    }

    pub fn get(&self, id: K) -> Option<&V>
    where
        K: DenseId,
    {
        self.values.get(id.index())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, V> {
        self.values.iter()
    }

    #[must_use]
    pub fn par_iter(&self) -> RayonIter<'_, V>
    where
        V: Sync,
    {
        self.values.par_iter()
    }
}

impl<K, V> From<Vec<V>> for DenseStorage<K, V> {
    fn from(values: Vec<V>) -> Self {
        Self::from_vec(values)
    }
}

impl<K, V> IntoIterator for DenseStorage<K, V> {
    type Item = V;
    type IntoIter = std::vec::IntoIter<V>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

impl<'a, K, V> IntoIterator for &'a DenseStorage<K, V> {
    type Item = &'a V;
    type IntoIter = std::slice::Iter<'a, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

impl<K: DenseId, V> Index<K> for DenseStorage<K, V> {
    type Output = V;

    fn index(&self, id: K) -> &Self::Output {
        &self.values[id.index()]
    }
}

impl<K: DenseId, V> IndexMut<K> for DenseStorage<K, V> {
    fn index_mut(&mut self, id: K) -> &mut Self::Output {
        &mut self.values[id.index()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Copy, Clone)]
    struct TestId(usize);

    impl DenseId for TestId {
        fn index(self) -> usize {
            self.0
        }
    }

    #[test]
    fn from_pairs_preserves_typed_indices() {
        let storage =
            DenseStorage::<TestId, _>::from_pairs([(TestId(0), "zero"), (TestId(1), "one")]);

        assert_eq!(storage[TestId(1)], "one");
    }

    #[test]
    #[should_panic(expected = "dense ID invariant violated")]
    fn from_pairs_rejects_sparse_ids() {
        let _ = DenseStorage::<TestId, _>::from_pairs([(TestId(0), "zero"), (TestId(2), "two")]);
    }

    #[test]
    #[should_panic(expected = "dense ID invariant violated")]
    fn contiguous_id_assertion_rejects_sparse_ids() {
        assert_contiguous_ids([TestId(0), TestId(2)]);
    }
}
