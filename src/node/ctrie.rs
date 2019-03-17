use crate::{
    node::{IndirectionNode, SingletonNode},
    Ctrie, Generation, Key, Value,
};
use crossbeam::epoch::Guard;
use std::{fmt::Debug, hash::BuildHasher};

#[derive(Clone)]
pub enum Branch<K, V> {
    Indirection(IndirectionNode<K, V>),
    Singleton(SingletonNode<K, V>),
}

/// A Ctrie node (C-node).
///
/// Maintains the invariant that the number of 1s in the bitmap is equal to the length of the
/// branch array.
#[derive(Clone)]
pub struct CtrieNode<K, V> {
    bitmap: u64,
    array: Vec<Branch<K, V>>,
    generation: Generation,
}

impl<K, V> CtrieNode<K, V>
where
    K: Key,
    V: Value,
{
    /// Creates a new C-node with the given bitmap, array, and generation.
    pub fn new(bitmap: u64, array: Vec<Branch<K, V>>, generation: Generation) -> Self {
        Self {
            bitmap,
            array,
            generation,
        }
    }

    /// Inserts a branch into the C-node, returning a new node.
    pub fn inserted(
        &self,
        flag: u64,
        position: usize,
        branch: Branch<K, V>,
        generation: Generation,
    ) -> Self {
        let mut new_array = self.array.clone();
        new_array.insert(position, branch);
        Self {
            bitmap: self.bitmap | flag,
            array: new_array,
            generation,
        }
    }

    /// Updates a position in the C-node with a new branch, returning a new node.
    pub fn updated(&self, position: usize, branch: Branch<K, V>, generation: Generation) -> Self {
        let mut new_array = self.array.clone();
        new_array[position] = branch;
        Self {
            array: new_array,
            bitmap: self.bitmap,
            generation,
        }
    }

    pub fn renewed<S: BuildHasher>(
        &self,
        generation: Generation,
        ctrie: &Ctrie<K, V, S>,
        guard: &Guard,
    ) -> Self {
        let mut new_array = Vec::with_capacity(self.array.len());
        for branch in &self.array {
            match branch {
                Branch::Indirection(inode) => new_array.push(Branch::Indirection(
                    inode.copy_to_generation(generation.clone(), ctrie, guard),
                )),
                Branch::Singleton(snode) => new_array.push(Branch::Singleton(snode.clone())),
            }
        }
        Self {
            array: new_array,
            bitmap: self.bitmap,
            generation: generation,
        }
    }

    pub fn branch(&self, position: usize) -> &Branch<K, V> {
        &self.array[position]
    }

    pub fn bitmap(&self) -> u64 {
        self.bitmap
    }

    pub fn generation(&self) -> &Generation {
        &self.generation
    }

    pub fn print<'g>(&self, indent: usize, guard: &'g Guard)
    where
        K: Debug,
        V: Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}cnode:", tab);
        println!("{}bitmap: {:064b}", tab, self.bitmap);
        println!("{}array:", tab);
        for branch in &self.array {
            match branch {
                Branch::Indirection(inode) => inode.print(indent + 2, guard),
                Branch::Singleton(snode) => snode.print(indent + 2),
            }
        }
    }
}
