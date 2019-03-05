use crossbeam::epoch::{self, Atomic, Guard, Shared};
use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::atomic::Ordering,
};

const W: u8 = 6;

struct Ctrie<K, V> {
    root: Atomic<InternalNode<K, V>>,
}

impl<K, V> Ctrie<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn new() -> Self {
        Self { root: Atomic::new(InternalNode::new(MainNode::Ctrie(Atomic::new(CtrieNode::new(0u64, vec![]))))) }
    }

    fn lookup<'g>(&self, guard: &'g Guard, key: &K) -> Option<&'g V>
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

    fn insert<'g>(&self, guard: &'g Guard, key: K, value: V) {
        let root = unsafe { self.root.load(Ordering::SeqCst, guard).deref() };
        match iinsert(guard, root, key.clone(), value.clone(), 0, None) {
            IInsertResult::Ok => {},
            IInsertResult::Restart => { self.insert(guard, key, value) },
        }
    }

    fn print<'g>(&self, guard: &'g Guard) where K: Debug, V: Debug {
        println!("root:"); 
        unsafe { self.root.load(Ordering::SeqCst, guard).deref() }.print(guard, 0);
    }
}

fn ilookup<'g, K, V>(
    guard: &'g Guard,
    internal: &InternalNode<K, V>,
    key: &K,
    level: u8,
    parent: Option<&InternalNode<K, V>>,
) -> ILookupResult<'g, V>
where
    K: 'g + Eq + Hash,
{
    let main = &internal.main;
    match main {
        MainNode::Ctrie(ctrie_node) => {
            let ctrie_node = unsafe { ctrie_node.load(Ordering::SeqCst, guard).deref() };
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let (flag, position) = flag_and_position(hash, level, ctrie_node.bitmap);
            if ctrie_node.bitmap & flag == 0 {
                ILookupResult::NotFound
            } else {
                match &ctrie_node.array[position as usize] {
                    Branch::Internal(new_internal) => {
                        ilookup(guard, new_internal, key, level + W, Some(internal))
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
    }
}

fn iinsert<'g, K, V>(
    guard: &'g Guard,
    internal: &InternalNode<K, V>,
    key: K,
    value: V,
    level: u8,
    parent: Option<&InternalNode<K, V>>,
) -> IInsertResult
where
    K: 'g + Clone + Eq + Hash,
    V: Clone,
{
    let main = &internal.main;
    match main {
        MainNode::Ctrie(ctrie_node_pointer) => {
            let ctrie_node = unsafe { ctrie_node_pointer.load(Ordering::SeqCst, guard).deref() };
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let (flag, position) = flag_and_position(hash, level, ctrie_node.bitmap);
            if ctrie_node.bitmap & flag == 0 {
                let new_ctrie_node =
                    ctrie_node.inserted(flag, position as usize, SingletonNode::new(key, value));
                let new_ctrie_node = Atomic::new(new_ctrie_node);
                if ctrie_node_pointer
                    .compare_and_set(
                        ctrie_node_pointer.load(Ordering::SeqCst, guard),
                        new_ctrie_node.load(Ordering::SeqCst, guard),
                        Ordering::SeqCst,
                        guard,
                    )
                    .is_ok()
                {
                    IInsertResult::Ok
                } else {
                    IInsertResult::Restart
                }
            } else {
                match &ctrie_node.array[position as usize] {
                    Branch::Internal(new_internal) => {
                        iinsert(guard, new_internal, key, value, level + W, Some(internal))
                    }
                    Branch::Singleton(singleton) => {
                        if singleton.key != key {
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            key.hash(&mut hasher);
                            let new_singleton_key_hash = hasher.finish();
                            let new_singleton_node = SingletonNode::new(key, value);

                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            singleton.key.hash(&mut hasher);
                            let singleton_key_hash = hasher.finish();

                            let new_internal_node =
                                Branch::Internal(InternalNode::new(MainNode::new(
                                    singleton.clone(),
                                    singleton_key_hash,
                                    new_singleton_node,
                                    new_singleton_key_hash,
                                    level + W,
                                )));
                            let new_ctrie_node = Atomic::new(
                                ctrie_node.updated(position as usize, new_internal_node),
                            );
                            if ctrie_node_pointer
                                .compare_and_set(
                                    ctrie_node_pointer.load(Ordering::SeqCst, guard),
                                    new_ctrie_node.load(Ordering::SeqCst, guard),
                                    Ordering::SeqCst,
                                    guard,
                                )
                                .is_ok()
                            {
                                IInsertResult::Ok
                            } else {
                                IInsertResult::Restart
                            }
                        } else {
                            let new_ctrie_node = Atomic::new(ctrie_node.updated(
                                position as usize,
                                Branch::Singleton(SingletonNode::new(key, value)),
                            ));
                            if ctrie_node_pointer
                                .compare_and_set(
                                    ctrie_node_pointer.load(Ordering::SeqCst, guard),
                                    new_ctrie_node.load(Ordering::SeqCst, guard),
                                    Ordering::SeqCst,
                                    guard,
                                )
                                .is_ok()
                            {
                                IInsertResult::Ok
                            } else {
                                IInsertResult::Restart
                            }
                        }
                    }
                }
            }
        }
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

#[derive(Clone)]
struct InternalNode<K, V> {
    main: MainNode<K, V>,
}

impl<K, V> InternalNode<K, V> where K: Clone, V: Clone {
    fn new(main: MainNode<K, V>) -> Self {
        Self { main }
    }

    fn print<'g>(&self, guard: &'g Guard, indent: usize) where K: Debug, V: Debug, {
        println!("{}internal:", std::iter::repeat(' ').take(indent).collect::<String>());
        match &self.main {
            MainNode::Ctrie(ctrie_node) => {
                unsafe { ctrie_node.load(Ordering::SeqCst, guard).deref() }.print(guard, indent);
            }
        }
    }
}

#[derive(Clone)]
enum MainNode<K, V> {
    Ctrie(Atomic<CtrieNode<K, V>>),
}

impl<K, V> MainNode<K, V>
where
    K: Clone,
    V: Clone,
{
    fn new(
        x: SingletonNode<K, V>,
        x_hash: u64,
        y: SingletonNode<K, V>,
        y_hash: u64,
        level: u8,
    ) -> Self {
        let x_index = (x_hash >> level) & 0x3f;
        let y_index = (y_hash >> level) & 0x3f;
        let bitmap = (1u64 << x_index) | (1u64 << y_index);

        match x_index.cmp(&y_index) {
            std::cmp::Ordering::Equal => {
                let main = MainNode::new(x, x_hash, y, y_hash, level + W);
                let internal = InternalNode::new(main);
                MainNode::Ctrie(Atomic::new(CtrieNode::new(
                    bitmap,
                    vec![Branch::Internal(internal)],
                )))
            }
            std::cmp::Ordering::Less => MainNode::Ctrie(Atomic::new(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(x), Branch::Singleton(y)],
            ))),
            std::cmp::Ordering::Greater => MainNode::Ctrie(Atomic::new(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(y), Branch::Singleton(x)],
            ))),
        }
    }
}

#[derive(Clone)]
struct CtrieNode<K, V> {
    bitmap: u64,
    array: Vec<Branch<K, V>>,
}

impl<K, V> CtrieNode<K, V>
where
    K: Clone,
    V: Clone,
{
    fn new(bitmap: u64, array: Vec<Branch<K, V>>) -> Self {
        Self { bitmap, array }
    }
    fn inserted(&self, flag: u64, position: usize, singleton: SingletonNode<K, V>) -> Self {
        let mut new = self.clone();
        new.array.insert(position, Branch::Singleton(singleton));
        new.bitmap |= flag;
        new
    }

    fn updated(&self, position: usize, branch: Branch<K, V>) -> Self {
        let mut new = self.clone();
        new.array[position] = branch;
        new
    }

    fn print<'g>(&self, guard: &'g Guard, indent: usize) where K: Debug, V: Debug, {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}ctrie:", tab);
        println!("{}bitmap: {:064b}", tab, self.bitmap);
        println!("{}array:", tab);
        for branch in &self.array {
            match branch {
                Branch::Singleton(singleton_node) => singleton_node.print(indent + 2),
                Branch::Internal(internal_node) => internal_node.print(guard, indent + 2),
            }
        }
    }
}

#[derive(Clone)]
struct SingletonNode<K, V> {
    key: K,
    value: V,
}

impl<K, V> SingletonNode<K, V> {
    fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    fn print(&self, indent: usize) where K: std::fmt::Debug, V: std::fmt::Debug {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        print!("{}singleton: ", tab);
        println!("({:?}, {:?})", self.key, self.value);
    }
}

#[derive(Clone)]
enum Branch<K, V> {
    Internal(InternalNode<K, V>),
    Singleton(SingletonNode<K, V>),
}

fn flag_and_position(hash: u64, level: u8, bitmap: u64) -> (u64, u8) {
    // 0x3f ends in six 1's
    // index is thus guaranteed to be less than 64
    let index = (hash >> level) & 0x3f;
    // to calculate the array position, count the number of 1's in the bitmap that precede index
    let flag = 1u64 << index;
    let position = (bitmap & (flag - 1)).count_ones() as u8;
    (flag, position)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let ctrie = Ctrie::<char, u8>::new();
        let guard = &epoch::pin();

        for i in 0..26 {
            ctrie.insert(guard, (i + 65) as char, i);
        }

        ctrie.print(guard);
        println!();
    }
}
