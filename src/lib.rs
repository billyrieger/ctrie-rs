use crossbeam::epoch::{Atomic, Guard};
use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::atomic::Ordering,
    sync::Arc,
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

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for Generation {}

struct TombNode<K, V> {
    singleton: SingletonNode<K, V>,
}

enum IInsertResult {
    Ok,
    Restart,
}

enum ILookupResult<'g, V> {
    Value(&'g V),
    NotFound,
    Restart,
}

impl<K, V> Ctrie<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    pub fn new() -> Self {
        Self {
            root: Atomic::new(IndirectionNode::new(Atomic::new(MainNode::Ctrie(
                CtrieNode::new(0u64, vec![]),
            )))),
            read_only: false,
        }
    }

    pub fn lookup<'g>(&self, guard: &'g Guard, key: &K) -> Option<&'g V>
    where
        K: 'g,
    {
        let root = unsafe { self.root.load(Ordering::SeqCst, guard).deref() };
        match ilookup(guard, root, key, 0, None) {
            ILookupResult::Value(v) => Some(v),
            ILookupResult::NotFound => None,
            ILookupResult::Restart => self.lookup(guard, key),
        }
    }

    pub fn insert<'g>(&self, guard: &'g Guard, key: K, value: V) {
        let root = unsafe { self.root.load(Ordering::SeqCst, guard).deref() };
        match iinsert(guard, root, key.clone(), value.clone(), 0, None) {
            IInsertResult::Ok => {}
            IInsertResult::Restart => self.insert(guard, key, value),
        }
    }

    pub fn print<'g>(&self, guard: &'g Guard)
    where
        K: Debug,
        V: Debug,
    {
        println!("root:");
        unsafe { self.root.load(Ordering::SeqCst, guard).deref() }.print(guard, 0);
    }
}

fn ilookup<'g, K, V>(
    guard: &'g Guard,
    indirection: &IndirectionNode<K, V>,
    key: &K,
    level: usize,
    parent: Option<&IndirectionNode<K, V>>,
) -> ILookupResult<'g, V>
where
    K: 'g + Clone + Eq + Hash,
    V: Clone,
{
    let main_pointer = indirection.load_main(Ordering::SeqCst, guard);
    let main = unsafe { main_pointer.deref() };
    match main {
        MainNode::Ctrie(ctrie_node) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let bitmap = ctrie_node.bitmap();
            let (flag, position) = flag_and_position(hash, level, bitmap);
            if bitmap & flag == 0 {
                ILookupResult::NotFound
            } else {
                match ctrie_node.get_branch(position) {
                    Branch::Indirection(new_indirection) => {
                        ilookup(guard, new_indirection, key, level + W, Some(indirection))
                    }
                    Branch::Singleton(singleton) => {
                        if singleton.key == *key {
                            ILookupResult::Value(&singleton.value)
                        } else {
                            ILookupResult::NotFound
                        }
                    }
                }
            }
        }
        MainNode::List(list_node) => {
            if let Some(value) = list_node.lookup(guard, key) {
                ILookupResult::Value(value)
            } else {
                ILookupResult::NotFound
            }
        }
    }
}

fn iinsert<'g, K, V>(
    guard: &'g Guard,
    indirection: &IndirectionNode<K, V>,
    key: K,
    value: V,
    level: usize,
    parent: Option<&IndirectionNode<K, V>>,
) -> IInsertResult
where
    K: 'g + Clone + Eq + Hash,
    V: Clone,
{
    let main_pointer = indirection.load_main(Ordering::SeqCst, guard);
    let main = unsafe { main_pointer.deref() };
    match main {
        MainNode::Ctrie(ctrie_node) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let bitmap = ctrie_node.bitmap();
            let (flag, position) = flag_and_position(hash, level, bitmap);
            if bitmap & flag == 0 {
                let new_ctrie_node =
                    ctrie_node.inserted(flag, position as usize, SingletonNode::new(key, value));
                let new_main_node = Atomic::new(MainNode::Ctrie(new_ctrie_node));
                if indirection.cas_main(main_pointer, new_main_node.load(Ordering::SeqCst, guard), Ordering::SeqCst, guard)
                {
                    IInsertResult::Ok
                } else {
                    IInsertResult::Restart
                }
            } else {
                match ctrie_node.get_branch(position) {
                    Branch::Indirection(new_indirection) => iinsert(
                        guard,
                        new_indirection,
                        key,
                        value,
                        level + W,
                        Some(indirection),
                    ),
                    Branch::Singleton(singleton) => {
                        let new_main_node = if singleton.key != key {
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            key.hash(&mut hasher);
                            let new_singleton_key_hash = hasher.finish();
                            let new_singleton_node = SingletonNode::new(key, value);

                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            singleton.key.hash(&mut hasher);
                            let singleton_key_hash = hasher.finish();

                            let new_indirection_node = Branch::Indirection(IndirectionNode::new(
                                Atomic::new(MainNode::new(
                                    singleton.clone(),
                                    singleton_key_hash,
                                    new_singleton_node,
                                    new_singleton_key_hash,
                                    level + W,
                                )),
                            ));
                            Atomic::new(MainNode::Ctrie(
                                ctrie_node.updated(position as usize, new_indirection_node),
                            ))
                        } else {
                            let new_ctrie_node = ctrie_node.updated(
                                position as usize,
                                Branch::Singleton(SingletonNode::new(key, value)),
                            );
                            Atomic::new(MainNode::Ctrie(new_ctrie_node))
                        };
                        if indirection.cas_main(main_pointer, new_main_node.load(Ordering::SeqCst, guard), Ordering::SeqCst, guard)
                        {
                            IInsertResult::Ok
                        } else {
                            IInsertResult::Restart
                        }
                    }
                }
            }
        }
        MainNode::List(list_node) => {
            if unimplemented!() {
                IInsertResult::Ok
            } else {
                IInsertResult::Restart
            }
        }
    }
}

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
    use super::*;
    use crossbeam::epoch;

    #[test]
    fn test() {
        let ctrie = Ctrie::<u64, u64>::new();
        let guard = &epoch::pin();

        for i in 0..100 {
            ctrie.insert(guard, i, i);
        }

        ctrie.print(guard);
        println!();
    }
}
