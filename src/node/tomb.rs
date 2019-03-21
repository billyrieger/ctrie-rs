use crate::node::SingletonNode;
use crate::{Key, Value};

#[derive(Clone)]
pub struct TombNode<K, V> {
    snode: SingletonNode<K, V>,
}

impl<K, V> TombNode<K, V> where K: Key, V: Value {
    pub fn new(snode: SingletonNode<K, V>) -> Self {
        Self { snode }
    }

    pub fn untombed(&self) -> SingletonNode<K, V> {
        self.snode.clone()
    }
}
