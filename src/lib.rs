use crossbeam::epoch::{Atomic, Guard, Owned};
use fxhash::FxHasher;
use std::{
    fmt::{self, Debug},
    hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
    sync::{atomic::Ordering, Arc},
};

mod gcas;
mod node;

use self::{gcas::*, node::*};

/// The ordering to use when loading atomic pointers.
const LOAD_ORD: Ordering = Ordering::Relaxed;

/// The ordering to use when storing atomic pointers.
const STORE_ORD: Ordering = Ordering::Relaxed;

/// The ordering to use when compare-and-swapping atomic pointers.
const CAS_ORD: (Ordering, Ordering) = (Ordering::Relaxed, Ordering::Relaxed);

const W: usize = 6;

/// Used to extract the last W = 6 bits of a hash.
const LAST_W_BITS: u64 = 0b_111111;

/// A trait to represent a key in a ctrie.
pub trait Key: Clone + Eq + Hash {}
impl<K> Key for K where K: Clone + Eq + Hash {}

/// A trait to represent a value in a ctrie.
pub trait Value: Clone {}
impl<V> Value for V where V: Clone {}

/// A heap-allocated counter to mark Ctrie snapshots.
/// It's possible to use a integer counter instead, but it could overflow.
#[derive(Clone)]
pub struct Generation {
    inner: Arc<()>,
}

impl Generation {
    /// Creates a new `Generation`.
    fn new() -> Self {
        Self {
            inner: Arc::new(()),
        }
    }
}

impl Debug for Generation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // debug representation is based on the pointer, not the value pointed to
        writeln!(f, "{:?}", Arc::into_raw(self.inner.clone()))
    }
}

impl Eq for Generation {}

impl PartialEq for Generation {
    fn eq(&self, other: &Self) -> bool {
        // generations are equal if their pointers are equal,
        // NOT if the values they point to are equal
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

fn flag_and_position(hash: u64, level: usize, bitmap: u64) -> (u64, usize) {
    // extract W = 6 bits from the hash, skipping the first level bits
    let index = (hash >> level) & LAST_W_BITS;

    // flag is the position in the bitmap corresponding to the index
    // index is guaranteed to be less than 2^W = 64, so this cannot overflow
    let flag = 1u64 << index;

    // to calculate the array position, count the number of 1's in the bitmap that precede index
    let position = (bitmap & (flag - 1)).count_ones() as usize;

    (flag, position)
}

pub fn entomb<K, V>(snode: SingletonNode<K, V>) -> MainNode<K, V> where K: Key, V: Value {
    MainNode::from_tomb_node(TombNode::new(snode))
}

pub fn resurrect<K, V>(inode: IndirectionNode<K, V>, main: &MainNode<K, V>) -> Branch<K, V> where K: Key, V: Value {
    match main.kind() {
        MainNodeKind::Tomb(tnode) => {
            Branch::Singleton(tnode.untombed())
        }
        _ => {
            Branch::Indirection(inode)
        }
    }
}

fn to_compressed<'g, K, V>(cnode: &CtrieNode<K, V>, level: u64, generation: Generation, guard: &'g Guard) -> MainNode<K, V>
where
    K: Key,
    V: Value,
{
    let mut new_array = Vec::with_capacity(cnode.branches());
    for i in 0..cnode.branches() {
        match cnode.branch(i) {
            Branch::Singleton(snode) => {
                new_array.push(Branch::Singleton(snode.clone()));
            }
            Branch::Indirection(inode) => {
                unimplemented!()
            }
        }
    }
    let new_cnode = CtrieNode::new(cnode.bitmap(), new_array, generation);
    new_cnode.to_contracted(level)
}

pub struct Ctrie<K, V, S = BuildHasherDefault<FxHasher>> {
    root: Atomic<IndirectionNode<K, V>>,
    read_only: bool,
    hash_builder: S,
}

impl<K, V> Ctrie<K, V>
where
    K: Key,
    V: Value,
{
    pub fn new() -> Self {
        Self::with_hasher(BuildHasherDefault::<FxHasher>::default())
    }
}

impl<K, V, S> Ctrie<K, V, S>
where
    K: Key,
    V: Value,
    S: BuildHasher,
{
    pub fn with_hasher(hash_builder: S) -> Self {
        let generation = Generation::new();
        Self {
            root: Atomic::new(IndirectionNode::new(
                Atomic::new(MainNode::from_ctrie_node(CtrieNode::new(
                    0,
                    vec![],
                    generation.clone(),
                ))),
                generation,
            )),
            read_only: false,
            hash_builder,
        }
    }

    fn hash(&self, key: &K) -> u64 {
        let mut hasher = self.hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn root(&self) -> &Atomic<IndirectionNode<K, V>> {
        &self.root
    }

    fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn insert<'g>(&self, key: K, value: V, guard: &'g Guard) {
        let root_ptr = self.root().load(LOAD_ORD, guard);
        let root = unsafe { root_ptr.deref() };
        match self.iinsert(
            root,
            key.clone(),
            value.clone(),
            0,
            root.generation(),
            guard,
        ) {
            IInsertResult::Ok => {}
            IInsertResult::Restart => self.insert(key, value, guard),
        }
    }

    fn iinsert<'g>(
        &self,
        inode: &IndirectionNode<K, V>,
        key: K,
        value: V,
        level: usize,
        start_generation: &Generation,
        guard: &'g Guard,
    ) -> IInsertResult {
        // read the main pointer of the i-node
        let main_ptr = gcas_read(inode, self, guard);
        let main = unsafe { main_ptr.deref() };

        match main.kind() {
            MainNodeKind::Ctrie(cnode) => {
                let bitmap = cnode.bitmap();
                let key_hash = self.hash(&key);
                let (flag, position) = flag_and_position(key_hash, level, bitmap);
                if flag & bitmap == 0 {
                    let renewed_cnode = if cnode.generation() != inode.generation() {
                        cnode.renewed(inode.generation().clone(), self, guard)
                    } else {
                        cnode.clone()
                    };
                    let new_main_ptr =
                        Owned::new(MainNode::from_ctrie_node(renewed_cnode.inserted(
                            flag,
                            position,
                            Branch::Singleton(SingletonNode::new(key, value)),
                            inode.generation().clone(),
                        )))
                        .into_shared(guard);
                    if gcas(inode, main_ptr, new_main_ptr, self, guard) {
                        IInsertResult::Ok
                    } else {
                        IInsertResult::Restart
                    }
                } else {
                    match cnode.branch(position) {
                        Branch::Indirection(inode) => {
                            if start_generation == inode.generation() {
                                self.iinsert(inode, key, value, level + W, start_generation, guard)
                            } else {
                                let renewed_cnode =
                                    cnode.renewed(start_generation.clone(), self, guard);
                                let new_main_ptr =
                                    Owned::new(MainNode::from_ctrie_node(renewed_cnode))
                                        .into_shared(guard);
                                if gcas(inode, main_ptr, new_main_ptr, self, guard) {
                                    self.iinsert(inode, key, value, level, start_generation, guard)
                                } else {
                                    IInsertResult::Restart
                                }
                            }
                        }
                        Branch::Singleton(snode) => {
                            if snode.key() != &key {
                                let renewed_cnode = if cnode.generation() != inode.generation() {
                                    cnode.renewed(inode.generation().clone(), self, guard)
                                } else {
                                    cnode.clone()
                                };
                                let new_snode = SingletonNode::new(key, value);
                                let new_main = MainNode::new(
                                    snode.clone(),
                                    self.hash(snode.key()),
                                    new_snode,
                                    key_hash,
                                    level + W,
                                    inode.generation().clone(),
                                );
                                let new_inode = IndirectionNode::new(
                                    Atomic::new(new_main),
                                    inode.generation().clone(),
                                );
                                let new_main_ptr =
                                    Owned::new(MainNode::from_ctrie_node(renewed_cnode.updated(
                                        position,
                                        Branch::Indirection(new_inode),
                                        inode.generation().clone(),
                                    )))
                                    .into_shared(guard);
                                if gcas(inode, main_ptr, new_main_ptr, self, guard) {
                                    IInsertResult::Ok
                                } else {
                                    IInsertResult::Restart
                                }
                            } else {
                                let new_main_ptr =
                                    Owned::new(MainNode::from_ctrie_node(cnode.updated(
                                        position,
                                        Branch::Singleton(SingletonNode::new(key, value)),
                                        inode.generation().clone(),
                                    )))
                                    .into_shared(guard);
                                if gcas(inode, main_ptr, new_main_ptr, self, guard) {
                                    IInsertResult::Ok
                                } else {
                                    IInsertResult::Restart
                                }
                            }
                        }
                    }
                }
            }

            MainNodeKind::List(lnode) => unimplemented!(),

            MainNodeKind::Tomb(tnode) => unimplemented!(),

            MainNodeKind::Failed => unimplemented!(),
        }
    }

    pub fn lookup<'g>(&self, key: &K, guard: &'g Guard) -> Option<&'g V>
    where
        K: 'g,
    {
        let root_ptr = self.root.load(LOAD_ORD, guard);
        let root = unsafe { root_ptr.deref() };
        match self.ilookup(root, key, 0, root.generation(), guard) {
            ILookupResult::Value(v) => Some(v),
            ILookupResult::NotFound => None,
            ILookupResult::Restart => self.lookup(key, guard),
        }
    }

    fn ilookup<'g>(
        &self,
        inode: &IndirectionNode<K, V>,
        key: &K,
        level: usize,
        start_generation: &Generation,
        guard: &'g Guard,
    ) -> ILookupResult<'g, V>
    where
        K: 'g,
    {
        // read the main pointer of the i-node
        let main_ptr = gcas_read(inode, self, guard);
        let main = unsafe { main_ptr.deref() };

        match main.kind() {
            MainNodeKind::Ctrie(cnode) => {
                // if the main node is a c-node, calculate the flag and array position
                // corresponding to the key
                let bitmap = cnode.bitmap();
                let key_hash = self.hash(&key);
                let (flag, position) = flag_and_position(key_hash, level, bitmap);

                if flag & bitmap == 0 {
                    // if the bitmap doesn't contain the relevant bit, the key is not present in
                    // the ctrie
                    ILookupResult::NotFound
                } else {
                    // otherwise, check the branch at the relevant position in the branch array
                    match cnode.branch(position) {
                        Branch::Indirection(new_inode) => {
                            if self.read_only || start_generation == new_inode.generation() {
                                self.ilookup(new_inode, key, level + W, start_generation, guard)
                            } else {
                                let new_main_ptr = Owned::new(MainNode::from_ctrie_node(
                                    cnode.renewed(start_generation.clone(), self, guard),
                                ))
                                .into_shared(guard);
                                if gcas(inode, main_ptr, new_main_ptr, self, guard) {
                                    self.ilookup(inode, key, level, start_generation, guard)
                                } else {
                                    ILookupResult::Restart
                                }
                            }
                        }
                        Branch::Singleton(snode) => {
                            // if the branch is a s-node, simply check if the keys match
                            if snode.key() == key {
                                ILookupResult::Value(snode.value())
                            } else {
                                ILookupResult::NotFound
                            }
                        }
                    }
                }
            }

            MainNodeKind::List(lnode) => {
                // if the main node is an l-node, lookup the key in the linked list
                if let Some(value) = lnode.lookup(key, guard) {
                    ILookupResult::Value(value)
                } else {
                    ILookupResult::NotFound
                }
            }

            MainNodeKind::Tomb(tnode) => unimplemented!(),

            MainNodeKind::Failed => unimplemented!(),
        }
    }

    fn print<'g>(&self, guard: &'g Guard)
    where
        K: Debug,
        V: Debug,
    {
        println!("ctrie:");
        let root_ptr = self.root.load(LOAD_ORD, guard);
        let root = unsafe { root_ptr.deref() };
        root.print(0, guard);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::epoch;

    #[test]
    fn insert_lookup() {
        let ctrie = Ctrie::new();
        let guard = &epoch::pin();

        for i in (0..1000).map(|i| i * 2) {
            ctrie.insert(i, i * 3, guard);
        }

        for i in (0..1000).map(|i| i * 2) {
            assert_eq!(ctrie.lookup(&i, guard), Some(&(i * 3)));
            assert_eq!(ctrie.lookup(&(i + 1), guard), None);
        }

        ctrie.print(guard);
    }
}
