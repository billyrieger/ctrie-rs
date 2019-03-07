use crate::node::{CtrieNode, ListNode, TombNode};
use crate::{Key, Value};
use crossbeam::epoch::Atomic;

#[derive(Clone)]
pub enum MainNodeKind<K, V> {
    Ctrie(CtrieNode<K, V>),
    List(ListNode<K, V>),
    Tomb(TombNode<K, V>),
    Failed,
}

#[derive(Clone)]
pub struct MainNode<K, V> {
    kind: MainNodeKind<K, V>,
    prev: Atomic<MainNode<K, V>>,
}

impl<K, V> MainNode<K, V> where K: Key, V: Value {
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

    pub fn from_tomb_node(tnode: TombNode<K, V>) -> Self {
        Self {
            kind: MainNodeKind::Tomb(tnode),
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
