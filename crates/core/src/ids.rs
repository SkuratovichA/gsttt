//! Dense ids, and the vectors and sets keyed by them.
//!
//! Each file and each contract gets a `u32` at index time. A query then walks flat arrays instead of hashing
//! paths, and a set of files is a bitset.

use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{Index, IndexMut};

use fixedbitset::FixedBitSet;
use serde::{Deserialize, Serialize};

/// A dense position in an [`IdVec`].
pub trait Id: Copy + Eq + Ord + Hash + fmt::Debug + Send + Sync + 'static {
    /// The id at `index`. An index past `u32::MAX` saturates; the engine refuses such repositories before it
    /// makes any id.
    fn from_index(index: usize) -> Self;
    /// The position this id stands for.
    fn index(self) -> usize;
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
            rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
        )]
        #[serde(transparent)]
        #[rkyv(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash))]
        pub struct $name(u32);

        impl $name {
            /// The raw number, for wire formats.
            #[must_use]
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// An id from its raw number. It means something only to the index that made it.
            #[must_use]
            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }
        }

        impl Id for $name {
            #[inline]
            fn from_index(index: usize) -> Self {
                Self(u32::try_from(index).unwrap_or(u32::MAX))
            }

            #[inline]
            fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

#[expect(
    missing_debug_implementations,
    reason = "rkyv derives a public resolver next to each id, and resolvers are never printed"
)]
mod id_types {
    use super::{Deserialize, Id, Serialize, fmt};

    define_id!(
        /// A file of the index. Files are numbered in path order, so the order of ids is the order of paths.
        FileId
    );

    define_id!(
        /// A contract of the index. Contracts are numbered in key order: by kind, in the order
        /// [`ContractKind`](crate::contract::ContractKind) declares them, then by name.
        ContractId
    );

    define_id!(
        /// A project of the index: one connector's view of one root, such as the TypeScript project in `apps/web`.
        ProjectId
    );
}

pub use id_types::{ContractId, FileId, ProjectId};

/// A vector indexed by a typed id, so a [`FileId`] cannot index a table of contracts.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdVec<I, T> {
    raw: Vec<T>,
    #[serde(skip)]
    _id: PhantomData<fn(I) -> I>,
}

impl<I: Id, T> IdVec<I, T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            raw: Vec::new(),
            _id: PhantomData,
        }
    }

    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            raw: Vec::with_capacity(capacity),
            _id: PhantomData,
        }
    }

    /// Appends `value` and returns its id.
    pub fn push(&mut self, value: T) -> I {
        let id = I::from_index(self.raw.len());
        self.raw.push(value);
        id
    }

    pub fn get(&self, id: I) -> Option<&T> {
        self.raw.get(id.index())
    }

    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        self.raw.get_mut(id.index())
    }

    pub fn len(&self) -> usize {
        self.raw.len()
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// The ids of this vector, in order.
    pub fn ids(&self) -> impl ExactSizeIterator<Item = I> + use<I, T> {
        (0..self.raw.len()).map(I::from_index)
    }

    /// Each value with its id.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (I, &T)> {
        self.raw
            .iter()
            .enumerate()
            .map(|(index, value)| (I::from_index(index), value))
    }

    pub fn values(&self) -> &[T] {
        &self.raw
    }

    pub fn into_values(self) -> Vec<T> {
        self.raw
    }
}

impl<I: Id, T> Default for IdVec<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Id, T> From<Vec<T>> for IdVec<I, T> {
    fn from(raw: Vec<T>) -> Self {
        Self {
            raw,
            _id: PhantomData,
        }
    }
}

impl<I: Id, T> FromIterator<T> for IdVec<I, T> {
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self::from(iter.into_iter().collect::<Vec<T>>())
    }
}

impl<I: Id, T> Index<I> for IdVec<I, T> {
    type Output = T;

    #[expect(
        clippy::indexing_slicing,
        reason = "an id is made by the container it indexes, so it is in bounds; a foreign id is a bug worth a panic"
    )]
    fn index(&self, id: I) -> &T {
        &self.raw[id.index()]
    }
}

impl<I: Id, T> IndexMut<I> for IdVec<I, T> {
    #[expect(
        clippy::indexing_slicing,
        reason = "an id is made by the container it indexes, so it is in bounds; a foreign id is a bug worth a panic"
    )]
    fn index_mut(&mut self, id: I) -> &mut T {
        &mut self.raw[id.index()]
    }
}

impl<I: Id, T: fmt::Debug> fmt::Debug for IdVec<I, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// A set of ids as a bitset: the visited set of a graph walk over a hundred thousand files is 12 KiB.
#[derive(Clone, PartialEq, Eq)]
pub struct IdSet<I> {
    bits: FixedBitSet,
    _id: PhantomData<fn(I) -> I>,
}

impl<I: Id> IdSet<I> {
    /// An empty set for ids below `universe`.
    #[must_use]
    pub fn with_universe(universe: usize) -> Self {
        Self {
            bits: FixedBitSet::with_capacity(universe),
            _id: PhantomData,
        }
    }

    /// Adds `id` and says whether it was new. The set grows for an id past its universe.
    pub fn insert(&mut self, id: I) -> bool {
        let index = id.index();
        if index >= self.bits.len() {
            self.bits.grow(index + 1);
        }
        !self.bits.put(index)
    }

    pub fn contains(&self, id: I) -> bool {
        self.bits.contains(id.index())
    }

    pub fn len(&self) -> usize {
        self.bits.count_ones(..)
    }

    pub fn is_empty(&self) -> bool {
        self.bits.is_clear()
    }

    /// The members in id order.
    pub fn iter(&self) -> impl Iterator<Item = I> + '_ {
        self.bits.ones().map(I::from_index)
    }
}

impl<I: Id> fmt::Debug for IdSet<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<I: Id> Extend<I> for IdSet<I> {
    fn extend<It: IntoIterator<Item = I>>(&mut self, iter: It) {
        for id in iter {
            self.insert(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_returns_dense_ids_in_order() {
        let mut files: IdVec<FileId, &str> = IdVec::new();
        let a = files.push("a.ts");
        let b = files.push("b.ts");
        assert_eq!((a.raw(), b.raw()), (0, 1));
        assert_eq!(files[b], "b.ts");
        assert_eq!(files.get(FileId::from_raw(2)), None);
    }

    #[test]
    fn set_insert_reports_novelty_and_grows() {
        let mut seen: IdSet<FileId> = IdSet::with_universe(2);
        assert!(seen.insert(FileId::from_raw(1)));
        assert!(!seen.insert(FileId::from_raw(1)));
        assert!(seen.insert(FileId::from_raw(70)));
        assert_eq!(
            seen.iter().map(FileId::raw).collect::<Vec<_>>(),
            vec![1, 70]
        );
    }
}
