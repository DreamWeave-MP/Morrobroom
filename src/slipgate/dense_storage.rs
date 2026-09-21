use std::{
    marker::PhantomData,
    ops::{Index, IndexMut},
};

use rayon::{iter::IntoParallelRefIterator, slice::Iter as RayonIter};

/// A dense, typed index into [`DenseStorage`].
pub trait DenseId: Copy {
    fn index(self) -> usize;
}

/// Contiguous storage indexed by a typed dense ID.
///
/// The type parameter prevents accidentally indexing one arena with another
/// arena's ID while keeping the representation a plain `Vec<T>`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct DenseStorage<K, V> {
    values: Vec<V>,
    marker: PhantomData<K>,
}

impl<K, V> DenseStorage<K, V> {
    pub fn from_vec(values: Vec<V>) -> Self {
        Self {
            values,
            marker: PhantomData,
        }
    }

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

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, V> {
        self.values.iter()
    }

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
