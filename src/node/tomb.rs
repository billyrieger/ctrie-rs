use crate::node::SingletonNode;

#[derive(Clone)]
pub struct TombNode<K, V> {
    snode: SingletonNode<K, V>,
}

impl<K, V> TombNode<K, V> {
    pub fn new(snode: SingletonNode<K, V>) -> Self {
        Self { snode }
    }
}
