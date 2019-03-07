use crate::node::{CtrieNode, ListNode, TombNode};
use crossbeam::epoch::Atomic;

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
