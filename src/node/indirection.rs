use crate::{
    node::{MainNode, MainNodeKind},
    Ctrie, Generation,
};
use crossbeam::epoch::{Atomic, Guard, Pointer, Shared};
use std::{fmt::Debug, hash::Hash, sync::atomic::Ordering};

#[derive(Clone)]
pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
}

impl<K, V> IndirectionNode<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    pub fn new(main: Atomic<MainNode<K, V>>) -> Self {
        Self {
            main,
            generation: Generation::new(),
        }
    }

    pub fn gcas_read_main<'g>(
        &self,
        ctrie: &Ctrie<K, V>,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> Shared<'g, MainNode<K, V>> {
        let main_pointer = self.main.load(ordering, guard);
        let main = unsafe { main_pointer.deref() };
        let prev_pointer = main.prev().load(ordering, guard);
        if prev_pointer.is_null() {
            main_pointer
        } else {
            self.gcas_commit(main_pointer, ctrie, ordering, guard)
        }
    }

    pub fn gcas_commit<'g>(
        &self,
        main_pointer: Shared<'g, MainNode<K, V>>,
        ctrie: &Ctrie<K, V>,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> Shared<'g, MainNode<K, V>> {
        let main = unsafe { main_pointer.deref() };
        let prev_pointer = main.prev().load(ordering, guard);

        let root_pointer = ctrie.root().load(ordering, guard);
        let root = unsafe { root_pointer.deref() };

        if prev_pointer.is_null() {
            main_pointer
        } else {
            let prev = unsafe { prev_pointer.deref() };
            match prev.kind() {
                MainNodeKind::Failed => {
                    let failed_prev = prev.prev().load(ordering, guard);
                    if self.main.compare_and_set(main_pointer, failed_prev, ordering, guard).is_ok() {
                        unimplemented!()
                    } else {
                        unimplemented!()
                    }
                }
                _ => {
                    if root.generation == self.generation && !ctrie.read_only() {
                        unimplemented!()
                    } else {
                        let failed = Atomic::new(MainNode::failed(main.prev().clone())).load(ordering, guard);
                        main.prev().compare_and_set(prev_pointer, failed, ordering, guard);
                        let new_main = self.main.load(ordering, guard);
                        self.gcas_commit(new_main, ctrie, ordering, guard)
                    }
                }
            }
        }
    }

    pub fn load_main<'g>(
        &self,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> Shared<'g, MainNode<K, V>> {
        self.main.load(ordering, guard)
    }

    pub fn cas_main<'g, P>(
        &self,
        current_main: Shared<MainNode<K, V>>,
        new: P,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> bool
    where
        P: Pointer<MainNode<K, V>>,
    {
        self.main
            .compare_and_set(current_main, new, ordering, guard)
            .is_ok()
    }

    pub fn print<'g>(&self, guard: &'g Guard, indent: usize)
    where
        K: Clone + Debug,
        V: Clone + Debug,
    {
        println!(
            "{}indirection:",
            std::iter::repeat(' ').take(indent).collect::<String>()
        );
        match unsafe { self.main.load(Ordering::SeqCst, guard).deref() }.kind() {
            MainNodeKind::Ctrie(ctrie_node) => {
                ctrie_node.print(guard, indent);
            }
            MainNodeKind::List(list_node) => unimplemented!(),
            MainNodeKind::Tomb(tnode) => unimplemented!(),
            MainNodeKind::Failed => unimplemented!(),
        }
    }
}
