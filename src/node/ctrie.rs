use crate::node::{IndirectionNode, SingletonNode};
use crossbeam::epoch::Guard;
use std::{fmt::Debug, hash::Hash};
use std::sync::atomic::Ordering;
use crate::{Generation, Ctrie};

#[derive(Clone)]
pub enum Branch<K, V> {
    Indirection(IndirectionNode<K, V>),
    Singleton(SingletonNode<K, V>),
}

#[derive(Clone)]
pub struct CtrieNode<K, V> {
    bitmap: u64,
    array: Vec<Branch<K, V>>,
}

impl<K, V> CtrieNode<K, V>
where
    K: Clone + Debug + Eq + Hash,
    V: Clone + Debug,
{
    pub fn new(bitmap: u64, array: Vec<Branch<K, V>>) -> Self {
        Self { bitmap, array }
    }

    pub fn bitmap(&self) -> u64 {
        self.bitmap
    }

    pub fn get_branch(&self, index: usize) -> &Branch<K, V> {
        &self.array[index]
    }

    pub fn inserted(&self, flag: u64, position: usize, singleton: SingletonNode<K, V>) -> Self {
        let mut new = self.clone();
        new.array.insert(position, Branch::Singleton(singleton));
        new.bitmap |= flag;
        new
    }

    pub fn updated(&self, position: usize, branch: Branch<K, V>) -> Self {
        let mut new = self.clone();
        new.array[position] = branch;
        new
    }

    pub fn renewed<'g>(&self, generation: Generation, ctrie: &Ctrie<K, V>, ordering: Ordering, guard: &'g Guard) -> Self {
        let mut new_array = Vec::with_capacity(self.array.len());
        for branch in &self.array {
            match branch {
                Branch::Indirection(inode) => new_array.push(Branch::Indirection(inode.copy_to_gen(generation.clone(), ctrie, ordering, guard))),
                Branch::Singleton(snode) => new_array.push(Branch::Singleton(snode.clone())),
            }
        }
        Self {
            array: new_array,
            bitmap: self.bitmap,
        }
    }
}

impl<K, V> CtrieNode<K, V> {
    pub fn print<'g>(&self, guard: &'g Guard, indent: usize)
    where
        K: Clone + Debug + Eq + Hash,
        V: Clone + Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}ctrie:", tab);
        println!("{}bitmap: {:064b}", tab, self.bitmap);
        println!("{}array:", tab);
        for branch in &self.array {
            match branch {
                Branch::Singleton(singleton_node) => singleton_node.print(indent + 2),
                Branch::Indirection(indirection_node) => indirection_node.print(guard, indent + 2),
            }
        }
    }
}
