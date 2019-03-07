use crate::{
    node::{IndirectionNode, MainNode, MainNodeKind},
    Ctrie, Key, Value, CAS_ORD, LOAD_ORD, STORE_ORD,
};
use crossbeam::epoch::{Atomic, Guard, Owned, Shared};
use std::hash::BuildHasher;

pub fn gcas<'g, K, V, S>(
    inode: &IndirectionNode<K, V>,
    old_ptr: Shared<MainNode<K, V>>,
    new_ptr: Shared<MainNode<K, V>>,
    ctrie: &Ctrie<K, V, S>,
    guard: &'g Guard,
) -> bool
where
    K: Key,
    V: Value,
    S: BuildHasher,
{
    let new = unsafe { new_ptr.deref() };

    // store the previous value in case we need to reset
    new.prev().store(old_ptr, STORE_ORD);

    if inode
        .main()
        .compare_and_set(old_ptr, new_ptr, CAS_ORD, guard)
        .is_ok()
    {
        gcas_commit(inode, new_ptr, ctrie, guard);
        new.prev().load(LOAD_ORD, guard).is_null()
    } else {
        false
    }
}

pub fn gcas_read<'g, K, V, S>(
    inode: &IndirectionNode<K, V>,
    ctrie: &Ctrie<K, V, S>,
    guard: &'g Guard,
) -> Shared<'g, MainNode<K, V>>
where
    K: Key,
    V: Value,
    S: BuildHasher,
{
    // load main
    let main_ptr = inode.main().load(LOAD_ORD, guard);

    // main pointer of inode is never null
    let main = unsafe { main_ptr.deref() };

    // load main.prev
    let main_prev_ptr = main.prev().load(LOAD_ORD, guard);
    if main_prev_ptr.is_null() {
        main_ptr
    } else {
        gcas_commit(inode, main_ptr, ctrie, guard)
    }
}

pub fn gcas_commit<'g, K, V, S>(
    inode: &IndirectionNode<K, V>,
    main_ptr: Shared<'g, MainNode<K, V>>,
    ctrie: &Ctrie<K, V, S>,
    guard: &'g Guard,
) -> Shared<'g, MainNode<K, V>>
where
    K: Key,
    V: Value,
    S: BuildHasher,
{
    // main pointer of inode is never null
    let main = unsafe { main_ptr.deref() };

    let prev_ptr = main.prev().load(LOAD_ORD, guard);

    // TODO: abortable read
    let root_ptr = ctrie.root().load(LOAD_ORD, guard);
    let root = unsafe { root_ptr.deref() };

    if prev_ptr.is_null() {
        main_ptr
    } else {
        // at this point prev_ptr is not null
        let prev = unsafe { prev_ptr.deref() };

        match prev.kind() {
            MainNodeKind::Failed => {
                let failed_prev_ptr = prev.prev().load(LOAD_ORD, guard);
                if inode
                    .main()
                    .compare_and_set(main_ptr, failed_prev_ptr, CAS_ORD, guard)
                    .is_ok()
                {
                    failed_prev_ptr
                } else {
                    let new_main_ptr = inode.main().load(LOAD_ORD, guard);
                    gcas_commit(inode, new_main_ptr, ctrie, guard)
                }
            }
            _ => {
                if root.generation() == inode.generation() && !ctrie.read_only() {
                    if main
                        .prev()
                        .compare_and_set(prev_ptr, Shared::null(), CAS_ORD, guard)
                        .is_ok()
                    {
                        main_ptr
                    } else {
                        gcas_commit(inode, main_ptr, ctrie, guard)
                    }
                } else {
                    let failed = MainNode::failed(Atomic::new(prev.clone()));
                    let failed_ptr = Owned::new(failed).into_shared(guard);
                    // TODO: ?
                    main.prev()
                        .compare_and_set(prev_ptr, failed_ptr, CAS_ORD, guard)
                        .is_ok();

                    let new_main_ptr = inode.main().load(LOAD_ORD, guard);
                    gcas_commit(inode, new_main_ptr, ctrie, guard)
                }
            }
        }
    }
}
