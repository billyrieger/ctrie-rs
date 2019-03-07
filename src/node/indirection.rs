use crate::{gcas::*, node::MainNode, Ctrie, Generation, Key, Value, LOAD_ORD};
use crossbeam::epoch::{Atomic, Guard};
use std::hash::BuildHasher;
use std::fmt::Debug;

pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
}

impl<K, V> IndirectionNode<K, V>
where
    K: Key,
    V: Value,
{
    pub fn new(main: Atomic<MainNode<K, V>>, generation: Generation) -> Self {
        Self { main, generation }
    }

    pub fn copy_to_generation<'g, S: BuildHasher>(
        &self,
        generation: Generation,
        ctrie: &Ctrie<K, V, S>,
        guard: &'g Guard,
    ) -> Self {
        let main = gcas_read(self, ctrie, guard);
        Self {
            main: Atomic::from(main),
            generation,
        }
    }

    pub fn main(&self) -> &Atomic<MainNode<K, V>> {
        &self.main
    }

    pub fn generation(&self) -> &Generation {
        &self.generation
    }

    pub fn print<'g>(&self, indent: usize, guard: &'g Guard) where K: Debug, V: Debug {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}inode:", tab);
        let main_ptr = self.main.load(LOAD_ORD, guard);
        let main = unsafe { main_ptr.deref() };
        main.print(indent, guard);
    }
}

impl<K, V> Clone for IndirectionNode<K, V> {
    fn clone(&self) -> Self {
        Self {
            // note: cloning an `Atomic` uses `Ordering::Relaxed`
            main: self.main.clone(),
            generation: self.generation.clone(),
        }
    }
}
