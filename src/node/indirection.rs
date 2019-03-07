use crate::{
    node::{MainNode, MainNodeKind},
    Ctrie, Generation,
};
use crossbeam::epoch::{Atomic, Guard, Owned, Pointer, Shared};
use std::{fmt::Debug, hash::Hash, sync::atomic::Ordering};

#[derive(Clone)]
pub struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
    generation: Generation,
}

impl<K, V> IndirectionNode<K, V>
where
    K: Clone + Debug + Eq + Hash,
    V: Clone + Debug,
{
    pub fn new(main: Atomic<MainNode<K, V>>) -> Self {
        Self {
            main,
            generation: Generation::new(),
        }
    }

    pub fn copy_to_gen<'g>(&self, generation: Generation, ctrie: &Ctrie<K, V>, ordering: Ordering, guard: &'g Guard) -> Self {
        let main = self.gcas_read_main(ctrie, ordering, guard);
        let new_main = Atomic::from(main);
        Self {
            main: new_main,
            generation
        }
    }

    pub fn generation(&self) -> &Generation {
        &self.generation
    }

    pub fn gcas_main<'g>(
        &self,
        old_pointer: Shared<'g, MainNode<K, V>>,
        new_pointer: Shared<'g, MainNode<K, V>>,
        ctrie: &Ctrie<K, V>,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> bool {
        // write the old value to new.prev, in case we have to reset
        let new = unsafe { new_pointer.deref() };
        new.prev().store(old_pointer, Ordering::SeqCst);

        // attempt to CAS
        if let Ok(new_pointer) =
            self.main
                .compare_and_set(old_pointer, new_pointer, ordering, guard)
        {
            // if CAS was successful, commit
            println!("successful");
            self.gcas_commit(new_pointer, ctrie, ordering, guard);
            new.prev().load(Ordering::SeqCst, guard).is_null()
        } else {
            println!("unsuccessful");
            false
        }
    }

    pub fn gcas_read_main<'g>(
        &self,
        ctrie: &Ctrie<K, V>,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> Shared<'g, MainNode<K, V>> {
        // read the main pointer from self
        // linearization point
        let main_pointer = self.main.load(ordering, guard);
        let main = unsafe { main_pointer.deref() };

        // if main.prev is null, we're good to go
        // otherwise, we need to help commit the proposed previous value
        let prev_pointer = main.prev().load(ordering, guard);
        if prev_pointer.is_null() {
            main_pointer
        } else {
            self.gcas_commit(main_pointer, ctrie, ordering, guard)
        }
    }

    /// Commits a GCAS operation.
    pub fn gcas_commit<'g>(
        &self,
        main_pointer: Shared<'g, MainNode<K, V>>,
        ctrie: &Ctrie<K, V>,
        ordering: Ordering,
        guard: &'g Guard,
    ) -> Shared<'g, MainNode<K, V>> {
        // load main.prev
        let main = unsafe { main_pointer.deref() };
        let prev_pointer = main.prev().load(ordering, guard);

        // load the ctrie root
        let root_pointer = ctrie.root().load(ordering, guard);
        let root = unsafe { root_pointer.deref() };

        // if main.prev is null, some other thread already committed the value
        // so we just return the main pointer as-is
        if prev_pointer.is_null() {
            main_pointer
        } else {
            // at this point, prev is not null
            let prev = unsafe { prev_pointer.deref() };
            match prev.kind() {
                // if prev is a failed node, then self.main is reset to the previous value from the
                // failed node
                MainNodeKind::Failed => {
                    // load the previous value from the failed node
                    let failed_prev = prev.prev().load(ordering, guard);
                    if let Ok(failed_prev) =
                        self.main
                            .compare_and_set(main_pointer, failed_prev, ordering, guard)
                    {
                        // if the CAS is successful, we return the newly set value
                        failed_prev
                    } else {
                        // if the CAS doesn't succeed, we need to retry after reloading self.main
                        let new_main = self.main.load(ordering, guard);
                        self.gcas_commit(new_main, ctrie, ordering, guard)
                    }
                }
                // at this point, prev is not a failed node
                _ => {
                    // check if the generation of the ctrie root matches the generation of self
                    if root.generation == self.generation && !ctrie.read_only() {
                        // the generations match
                        if main
                            .prev()
                            .compare_and_set(prev_pointer, Shared::null(), ordering, guard)
                            .is_ok()
                        {
                            main_pointer
                        } else {
                            self.gcas_commit(main_pointer, ctrie, ordering, guard)
                        }
                    } else {
                        // the generations don't match
                        // store a failed node on main.prev to signal that the value needs to be
                        // reset
                        let failed = Owned::new(MainNode::failed(main.prev().clone()));
                        assert!(main
                            .prev()
                            .compare_and_set(prev_pointer, failed, ordering, guard)
                            .is_ok());
                        // reload main and try again
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
