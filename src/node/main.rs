use crate::node::SingletonNode;
use crate::node::CtrieNode;
use crate::node::ListNode;
use crate::node::IndirectionNode;
use crate::node::Branch;
use crossbeam::epoch::{Atomic};
use crate::W;

#[derive(Clone)]
pub enum MainNode<K, V> {
    Ctrie(CtrieNode<K, V>),
    List(ListNode<K, V>),
}

impl<K, V> MainNode<K, V>
where
    K: Clone,
    V: Clone,
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
