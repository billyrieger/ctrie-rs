use crossbeam::epoch::{Atomic, Guard, Owned};
use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::{atomic::Ordering, Arc},
};

mod node;

use self::node::*;

const W: usize = 6;

pub struct Ctrie<K, V> {
    root: Atomic<IndirectionNode<K, V>>,
    read_only: bool,
}

#[derive(Clone)]
struct Generation {
    inner: Arc<u8>,
}

impl Debug for Generation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "{:?}", Arc::into_raw(self.inner.clone()))
    }
}

impl Generation {
    fn new() -> Self {
        Self {
            // answer to the ultimate question of life, the universe, and everything
            inner: Arc::new(42),
        }
    }
}

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Generation {}

fn flag_and_position(hash: u64, level: usize, bitmap: u64) -> (u64, usize) {
    // 0x3f ends in six 1's
    // index is thus guaranteed to be less than 64
    let index = (hash >> level) & 0x3f;
    // to calculate the array position, count the number of 1's in the bitmap that precede index
    let flag = 1u64 << index;
    let position = (bitmap & (flag - 1)).count_ones() as usize;
    (flag, position)
}

#[cfg(test)]
mod tests {
}
