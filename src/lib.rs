use crossbeam::epoch::Atomic;
use fxhash::FxHasher;
use std::{
    fmt::{self, Debug},
    hash::{BuildHasherDefault, Hash},
    sync::{atomic::Ordering, Arc},
};

mod node;

use self::node::*;

const LOAD_ORD: Ordering = Ordering::Relaxed;
const STORE_ORD: Ordering = Ordering::Relaxed;

const W: usize = 6;

/// Used to extract the last W = 6 bits of a hash.
const LAST_W_BITS: u64 = 0b_111111;

pub trait Key: Clone + Eq + Hash {}
impl<K> Key for K where K: Clone + Eq + Hash {}

pub trait Value: Clone {}
impl<V> Value for V where V: Clone {}

pub struct Ctrie<K, V, S = BuildHasherDefault<FxHasher>> {
    root: Atomic<IndirectionNode<K, V>>,
    read_only: bool,
    hash_builder: S,
}

/// A heap-allocated counter to mark Ctrie snapshots.
/// It's possible to use a integer counter instead, but it could overflow.
#[derive(Clone)]
struct Generation {
    inner: Arc<()>,
}

impl Generation {
    /// Creates a new `Generation`.
    fn new() -> Self {
        Self {
            inner: Arc::new(()),
        }
    }
}

impl Debug for Generation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // debug representation is based on the pointer, not the value pointed to
        writeln!(f, "{:?}", Arc::into_raw(self.inner.clone()))
    }
}

impl Eq for Generation {}

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        // generations are equal if their pointers are equal,
        // NOT if the values they point to are equal
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

fn flag_and_position(hash: u64, level: usize, bitmap: u64) -> (u64, usize) {
    // extract W = 6 bits from the hash, skipping the first `level` bits
    let index = (hash >> level) & LAST_W_BITS;

    // to calculate the array position, count the number of 1's in the bitmap that precede index
    let flag = 1u64 << index;
    let position = (bitmap & (flag - 1)).count_ones() as usize;
    (flag, position)
}

#[cfg(test)]
mod tests {}
