use crate::node::MainNode;
use crossbeam::epoch::{Atomic, Guard, Shared, Pointer};
use std::fmt::Debug;
use std::sync::atomic::Ordering;

#[derive(Clone)]
pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
}

impl<K, V> IndirectionNode<K, V> {
    pub fn new(main: Atomic<MainNode<K, V>>) -> Self {
        Self { main }
    }

    pub fn load_main<'g>(&self, ordering: Ordering, guard: &'g Guard) -> Shared<'g, MainNode<K, V>> {
        self.main.load(ordering, guard)
    }

    pub fn cas_main<'g, P>(&self, current_main: Shared<MainNode<K, V>>, new: P, ordering: Ordering, guard: &'g Guard) -> bool where P: Pointer<MainNode<K, V>> {
        self.main.compare_and_set(current_main, new, ordering, guard).is_ok()
    }

    pub fn print<'g>(&self, guard: &'g Guard, indent: usize)
    where
        K: Debug,
        V: Debug,
    {
        println!(
            "{}indirection:",
            std::iter::repeat(' ').take(indent).collect::<String>()
        );
        match unsafe { self.main.load(Ordering::SeqCst, guard).deref() } {
            MainNode::Ctrie(ctrie_node) => {
                ctrie_node.print(guard, indent);
            }
            MainNode::List(list_node) => unimplemented!(),
        }
    }
}
