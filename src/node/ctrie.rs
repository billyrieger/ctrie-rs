use crate::node::{IndirectionNode, SingletonNode};

#[derive(Clone)]
pub enum Branch<K, V> {
    Indirection(IndirectionNode<K, V>),
    Singleton(SingletonNode<K, V>),
}

#[derive(Clone)]
pub struct CtrieNode<K, V> {
    bitmap: u64,
    array: Vec<Branch<K, V>>,
}
