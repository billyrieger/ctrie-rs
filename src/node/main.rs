use crate::{
    node::{Branch, CtrieNode, IndirectionNode, ListNode, SingletonNode, TombNode},
    W,
};
use crossbeam::epoch::Atomic;
use std::hash::Hash;
use std::fmt::Debug;

#[derive(Clone)]
pub enum MainNodeKind<K, V> {
    Ctrie(CtrieNode<K, V>),
    List(ListNode<K, V>),
    Tomb(TombNode<K, V>),
    Failed,
}

pub struct MainNode<K, V> {
    kind: MainNodeKind<K, V>,
    prev: Atomic<MainNode<K, V>>,
}

impl<K, V> MainNode<K, V>
where
    K: Clone + Debug + Eq + Hash,
    V: Clone + Debug,
{
    pub fn new(
        x: SingletonNode<K, V>,
        x_hash: u64,
        y: SingletonNode<K, V>,
        y_hash: u64,
        level: usize,
    ) -> Self {
        let x_index = (x_hash >> level) & 0x3f;
        let y_index = (y_hash >> level) & 0x3f;
        let bitmap = (1u64 << x_index) | (1u64 << y_index);

        match x_index.cmp(&y_index) {
            std::cmp::Ordering::Equal => {
                let main = Atomic::new(MainNode::new(x, x_hash, y, y_hash, level + W));
                let indirection = IndirectionNode::new(main);
                Self::from_ctrie_node(CtrieNode::new(
                    bitmap,
                    vec![Branch::Indirection(indirection)],
                ))
            }
            std::cmp::Ordering::Less => Self::from_ctrie_node(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(x), Branch::Singleton(y)],
            )),
            std::cmp::Ordering::Greater => Self::from_ctrie_node(CtrieNode::new(
                bitmap,
                vec![Branch::Singleton(y), Branch::Singleton(x)],
            )),
        }
    }

    pub fn failed(prev: Atomic<MainNode<K, V>>) -> Self {
        Self {
            kind: MainNodeKind::Failed,
            prev,
        }
    }

    pub fn from_ctrie_node(cnode: CtrieNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::Ctrie(cnode),
            prev: Atomic::null(),
        }
    }

    pub fn from_list_node(lnode: ListNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::List(lnode),
            prev: Atomic::null(),
        }
    }

    pub fn entomb(snode: SingletonNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::Tomb(TombNode::new(snode)),
            prev: Atomic::null(),
        }
    }

    pub fn kind(&self) -> &MainNodeKind<K, V> {
        &self.kind
    }

    pub fn prev(&self) -> &Atomic<MainNode<K, V>> {
        &self.prev
    }
}
