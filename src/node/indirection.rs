use crate::{node::MainNode, Generation};
use crossbeam::epoch::Atomic;

pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
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
