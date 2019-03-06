use crossbeam::epoch::{self, Atomic, Guard};
use std::{
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::atomic::Ordering,
    sync::Arc,
};

const W: u8 = 6;

struct Ctrie<K, V> {
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

#[derive(Clone)]
struct IndirectionNode<K, V> {
    main: Atomic<MainNode<K, V>>,
}

#[derive(Clone)]
enum MainNode<K, V> {
    Ctrie(CtrieNode<K, V>),
    List(ListNode<K, V>),
}

#[derive(Clone)]
struct CtrieNode<K, V> {
    bitmap: u64,
    array: Vec<Branch<K, V>>,
}

#[derive(Clone)]
struct SingletonNode<K, V> {
    key: K,
    value: V,
}

struct TombNode<K, V> {
    singleton: SingletonNode<K, V>,
}

#[derive(Clone)]
struct ListNode<K, V> {
    singleton: SingletonNode<K, V>,
    next: Option<Atomic<ListNode<K, V>>>,
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
    fn new() -> Self {
        Self {
            root: Atomic::new(IndirectionNode::new(Atomic::new(MainNode::Ctrie(
                CtrieNode::new(0u64, vec![]),
            )))),
            read_only: false,
        }
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
            IInsertResult::Ok => {}
            IInsertResult::Restart => self.insert(guard, key, value),
        }
    }

    fn print<'g>(&self, guard: &'g Guard)
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
    level: u8,
    parent: Option<&IndirectionNode<K, V>>,
) -> ILookupResult<'g, V>
where
    K: 'g + Eq + Hash,
{
    let main_pointer = indirection.main.load(Ordering::SeqCst, guard);
    let main = unsafe { main_pointer.deref() };
    match main {
        MainNode::Ctrie(ctrie_node) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let (flag, position) = flag_and_position(hash, level, ctrie_node.bitmap);
            if ctrie_node.bitmap & flag == 0 {
                ILookupResult::NotFound
            } else {
                match &ctrie_node.array[position as usize] {
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
    level: u8,
    parent: Option<&IndirectionNode<K, V>>,
) -> IInsertResult
where
    K: 'g + Clone + Eq + Hash,
    V: Clone,
{
    let main_pointer = indirection.main.load(Ordering::SeqCst, guard);
    let main = unsafe { main_pointer.deref() };
    match main {
        MainNode::Ctrie(ctrie_node) => {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            let hash = hasher.finish();
            let (flag, position) = flag_and_position(hash, level, ctrie_node.bitmap);
            if ctrie_node.bitmap & flag == 0 {
                let new_ctrie_node =
                    ctrie_node.inserted(flag, position as usize, SingletonNode::new(key, value));
                let new_main_node = Atomic::new(MainNode::Ctrie(new_ctrie_node));
                if indirection
                    .main
                    .compare_and_set(
                        main_pointer,
                        new_main_node.load(Ordering::SeqCst, guard),
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
                        if indirection
                            .main
                            .compare_and_set(
                                main_pointer,
                                new_main_node.load(Ordering::SeqCst, guard),
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
        MainNode::List(list_node) => {
            if unimplemented!() {
                IInsertResult::Ok
            } else {
                IInsertResult::Restart
            }
        }
    }
}

impl<K, V> IndirectionNode<K, V>
where
    K: Clone,
    V: Clone,
{
    fn new(main: Atomic<MainNode<K, V>>) -> Self {
        Self { main }
    }

    fn print<'g>(&self, guard: &'g Guard, indent: usize)
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
            MainNode::List(list_node) => {
                unimplemented!()
            }
        }
    }
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
                let main = Atomic::new(MainNode::new(x, x_hash, y, y_hash, level + W));
                let indirection = IndirectionNode::new(main);
                MainNode::Ctrie(CtrieNode::new(
                    bitmap,
                    vec![Branch::Indirection(indirection)],
                ))
            }
            std::cmp::Ordering::Less => MainNode::Ctrie(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(x), Branch::Singleton(y)],
            )),
            std::cmp::Ordering::Greater => MainNode::Ctrie(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(y), Branch::Singleton(x)],
            )),
        }
    }
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

    fn print<'g>(&self, guard: &'g Guard, indent: usize)
    where
        K: Debug,
        V: Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        println!("{}ctrie:", tab);
        println!("{}bitmap: {:064b}", tab, self.bitmap);
        println!("{}array:", tab);
        for branch in &self.array {
            match branch {
                Branch::Singleton(singleton_node) => singleton_node.print(indent + 2),
                Branch::Indirection(indirection_node) => indirection_node.print(guard, indent + 2),
            }
        }
    }
}

impl<K, V> SingletonNode<K, V> {
    fn new(key: K, value: V) -> Self {
        Self { key, value }
    }

    fn print(&self, indent: usize)
    where
        K: std::fmt::Debug,
        V: std::fmt::Debug,
    {
        let tab = std::iter::repeat(' ').take(indent).collect::<String>();
        print!("{}singleton: ", tab);
        println!("({:?}, {:?})", self.key, self.value);
    }
}

impl<K, V> ListNode<K, V> where K: Eq {
    fn lookup<'g>(&'g self, guard: &'g Guard, key: &K) -> Option<&'g V> {
        if self.singleton.key == *key {
            Some(&self.singleton.value)
        } else {
            if let Some(next) = self.next {
                unsafe { next.load(Ordering::SeqCst, guard).deref() }.lookup(guard, key)
            } else {
                None
            }
        }
    }
}

#[derive(Clone)]
enum Branch<K, V> {
    Indirection(IndirectionNode<K, V>),
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
        let ctrie = Ctrie::<u64, u64>::new();
        let guard = &epoch::pin();

        for i in 0..100 {
            ctrie.insert(guard, i, i);
        }

        ctrie.print(guard);
        println!();
    }
}
