use crate::{node::MainNode, Generation, Key, Value};
use crossbeam::epoch::Atomic;

pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
}

impl<K, V> IndirectionNode<K, V>
where
    K: Key,
    V: Value,
{
    pub fn main(&self) -> &Atomic<MainNode<K, V>> {
        &self.main
    }

    pub fn generation(&self) -> &Generation {
        &self.generation
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
